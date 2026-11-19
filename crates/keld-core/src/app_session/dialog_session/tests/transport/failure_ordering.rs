use super::{
    Arc, CorrelationId, DialogUiOutcome, Duration, GuardianOwnerCommand, PrimaryCallerContext,
    WebviewId, options, original, with_manifest,
};

#[test]
fn collision_owner_failure_is_fatal_even_after_reader_revocation() {
    for disconnected in [false, true] {
        let (mut handle, _peer, owner_rx, ui_rx) =
            crate::app_session::tests::initial_ready_router_with_window_loop();
        let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
        let dialog = Arc::new(dialog);
        handle.dialog = Some(Arc::clone(&dialog));
        dialog
            .reserve(original(7, 9), options(), WebviewId(3))
            .unwrap();
        let caller = {
            let current = handle.current.lock().unwrap();
            let active = current.as_ref().unwrap();
            PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            }
        };
        let acknowledge = std::thread::spawn(move || {
            let GuardianOwnerCommand::FailGeneration(attempt, reply) =
                owner_rx.recv_timeout(Duration::from_secs(3)).unwrap()
            else {
                panic!("wrong command")
            };
            assert_eq!(attempt, 1);
            if !disconnected {
                reply
                    .send(Err("injected collision recovery failure".to_owned()))
                    .unwrap();
            }
        });
        let error = handle
            .dispatch_dialog(&caller, CorrelationId(7), &[0xff])
            .unwrap_err()
            .to_string();
        acknowledge.join().unwrap();
        assert!(error.contains("KELD-IPC-005"), "{error}");
        assert!(
            error.contains(if disconnected {
                "guardian owner ended before acknowledging link failure"
            } else {
                "injected collision recovery failure"
            }),
            "{error}"
        );
        assert!(handle.current.lock().unwrap().is_none());
        assert!(
            caller
                .reader_stop
                .load(std::sync::atomic::Ordering::Acquire)
        );
        assert_eq!(
            ui_rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            crate::app_session::AppWindowCommand::Fatal
        );
    }
}

#[test]
fn terminal_custody_is_not_consumed_before_writer_lock_acquisition() {
    let (mut handle, _peer, _owner_rx, _ui_rx) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let dialog = Arc::new(dialog);
    handle.dialog = Some(Arc::clone(&dialog));
    let owner = original(7, 9);
    dialog.reserve(owner, options(), WebviewId(3)).unwrap();
    dialog.take_work().unwrap();
    dialog
        .record_native_outcome(owner, DialogUiOutcome::Dismissed, None)
        .unwrap();
    // Deterministically deny writer acquisition. Restoring complete-before-lock
    // consumes the terminal and fails the custody assertions below. This is a
    // lock-order fault discriminator, not a scheduler/native race qualification.
    let current = Arc::clone(&handle.current);
    assert!(
        std::thread::spawn(move || {
            let _writer = current.lock().unwrap();
            panic!("inject poisoned generation writer");
        })
        .join()
        .is_err()
    );
    let error = handle
        .dialog_native_finished(1, 9, DialogUiOutcome::Dismissed)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("generation writer lock poisoned")
    );
    assert_eq!(
        dialog.pending_original_for(owner.identity, owner.attempt),
        Some(CorrelationId(7))
    );
    assert!(!dialog.fully_drained());
}
