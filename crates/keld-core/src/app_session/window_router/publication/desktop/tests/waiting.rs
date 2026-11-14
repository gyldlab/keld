use super::*;
use std::sync::mpsc;
use std::thread;

#[test]
fn desktop_replay_waits_on_actual_value_position_outside_all_canonical_guards() {
    fixture(|router, caller, _, peer, writer, _, _| {
        select(router);
        subscribe(router, caller, 121);
        let occupied = writer
            .reserve_value(
                caller,
                &router.shutdown,
                Instant::now() + Duration::from_secs(2),
            )
            .expect("the actual sole VALUE position");
        let (notice, observed_wait) = mpsc::sync_channel(1);
        writer.observe_next_capacity_wait_for_test(notice);
        let source = router.clone();
        let thread = thread::spawn(move || {
            source.window_native_event(AppWindowEvent::DesktopObserved {
                facts: Some(observed()),
            })
        });
        let waited = observed_wait.recv_timeout(Duration::from_secs(1));
        let unlocked = router.shutdown.transition.try_lock().is_ok()
            && router.current.try_lock().is_ok()
            && router
                .windows
                .as_ref()
                .expect("Registry")
                .try_lock()
                .is_ok();
        drop(occupied);
        let result = thread.join().expect("actual source joined");
        assert!(
            waited.is_ok(),
            "the real VALUE capacity wait must be reached"
        );
        assert!(
            unlocked,
            "canonical guard cannot cross physical capacity waiting"
        );
        assert!(matches!(
            result.expect("actual write"),
            NativePublicationDisposition::Handled
        ));
        fact(peer, &expected(1, true));
        subscribed(peer, 121);
        ping(router, caller, peer);
    });
}

#[test]
fn late_native_fulfillment_keeps_the_original_subscribe_clock() {
    fixture(|router, caller, _, _, writer, _, commands| {
        select(router);
        let deadline = Instant::now() + Duration::from_millis(200);
        router
            .subscribe_window_until(
                caller,
                CorrelationId(131),
                keld_ipc::channel_table::WINDOW.wire_id(),
                deadline,
            )
            .expect("same pending call with its original clock");
        let occupied = writer
            .reserve_value(
                caller,
                &router.shutdown,
                Instant::now() + Duration::from_secs(2),
            )
            .expect("actual VALUE position");
        let (notice, observed_wait) = mpsc::sync_channel(1);
        writer.observe_next_capacity_wait_for_test(notice);
        let source = router.clone();
        let started = Instant::now();
        let thread = thread::spawn(move || {
            source.window_native_event(AppWindowEvent::DesktopObserved {
                facts: Some(observed()),
            })
        });
        let waited = observed_wait.recv_timeout(Duration::from_secs(1));
        let result = thread
            .join()
            .expect("original deadline ends the source wait");
        drop(occupied);
        assert!(
            waited.is_ok(),
            "test must reach the real wait before its original deadline"
        );
        let NativePublicationDisposition::FatalQueued(cause) =
            result.expect("existing final-fence native failure disposition")
        else {
            panic!("current never-admitted deadline failure must retain its typed cause");
        };
        assert!(cause.to_string().contains("deadline"), "{cause}");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "native receipt must not start a new 5 s clock"
        );
        assert!(matches!(
            commands.try_recv(),
            Ok(keld_wv::AppWindowCommand::Fatal)
        ));
        assert!(
            commands.try_recv().is_err(),
            "no duplicate Fatal or fabricated Ready"
        );
    });
}

#[test]
fn desktop_epoch_exhaustion_is_fenced_before_any_source_commit() {
    fixture(|router, _, _, _, _, _, commands| {
        select(router);
        router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("source")
            .desktop_epoch
            .0 = u64::MAX;
        let result = router
            .window_native_event(AppWindowEvent::DesktopObserved {
                facts: Some(observed()),
            })
            .expect("fenced failure disposition");
        assert!(matches!(
            result,
            NativePublicationDisposition::FatalQueued(_)
        ));
        let session = router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("source");
        assert_eq!(session.desktop_epoch.0, u64::MAX);
        assert!(!session.desktop_seen);
        assert!(session.desktop_latest.is_none());
        drop(session);
        assert!(matches!(
            commands.try_recv(),
            Ok(keld_wv::AppWindowCommand::Fatal)
        ));
        assert!(commands.try_recv().is_err());
    });
}
