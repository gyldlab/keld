use super::*;
use crate::app_session::{
    AppReadyTrigger, InitialMilestoneReceive, coordinate_window_events,
    receive_initial_milestone_with_native_order,
};
use std::sync::{OnceLock, mpsc};

#[test]
fn selected_initial_milestone_requires_the_actual_preceding_desktop_event() {
    fixture(|router, _, _, _, _, _, commands| {
        select(router);
        let (send, events) = mpsc::channel();
        send.send(AppWindowEvent::HostInitialized {
            observed_at: Instant::now(),
        })
        .expect("actual coordinator input");
        drop(send);
        let failure = coordinate_window_events(
            &events,
            router,
            &router.window_commands,
            AppReadyTrigger::HostInitialized,
            |_| panic!("missing preceding sample cannot publish Ready"),
            None,
        )
        .expect_err("initial ordering is mandatory for selected capability");
        assert!(
            failure
                .to_string()
                .contains("preceding native desktop sample")
        );
        assert!(
            !router
                .window_ready
                .load(std::sync::atomic::Ordering::Acquire)
        );
        assert!(matches!(
            commands.try_recv(),
            Ok(keld_wv::AppWindowCommand::Fatal)
        ));
        assert!(commands.try_recv().is_err());
    });
}

#[test]
fn scoped_refresh_before_init_cannot_mutate_epoch_or_publish_ready() {
    fixture(|router, caller, _, _, _, _, commands| {
        select(router);
        let (send, events) = mpsc::channel();
        send.send(AppWindowEvent::WindowsDesktopObserved {
            capture: AppDesktopCapture::from_current(
                caller.attempt,
                Arc::clone(&caller.reader_stop),
            ),
            facts: Some(observed()),
        })
        .expect("actual coordinator input");
        drop(send);
        let failure = coordinate_window_events(
            &events,
            router,
            &router.window_commands,
            AppReadyTrigger::HostInitialized,
            |_| panic!("refresh cannot become first Init/Ready"),
            None,
        )
        .expect_err("refresh must follow actual Init");
        assert!(
            failure
                .to_string()
                .contains("refresh outside initialized facade")
        );
        let session = router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("source");
        assert_eq!(session.desktop_epoch.0, 0);
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

#[test]
fn one_queued_initial_sample_preserves_original_native_timestamp_but_cannot_renew_watch() {
    let watch = OnceLock::new();
    let deadline = Instant::now();
    let observed_at = deadline
        .checked_sub(Duration::from_millis(1))
        .expect("observed earlier");
    watch
        .set(deadline)
        .expect("one existing absolute Init watch");
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::DesktopObserved { facts: None })
        .expect("first DTO");
    send.send(AppWindowEvent::HostInitialized { observed_at })
        .expect("on-time Init");
    assert!(matches!(
        receive_initial_milestone_with_native_order(&events, Some(&watch), true),
        InitialMilestoneReceive::Event(AppWindowEvent::DesktopObserved { .. })
    ));
    assert!(matches!(
        receive_initial_milestone_with_native_order(&events, Some(&watch), false),
        InitialMilestoneReceive::Event(AppWindowEvent::HostInitialized { observed_at: actual })
            if actual == observed_at
    ));
    send.send(AppWindowEvent::DesktopObserved { facts: None })
        .expect("duplicate DTO");
    assert!(matches!(
        receive_initial_milestone_with_native_order(&events, Some(&watch), false),
        InitialMilestoneReceive::UnexpectedDesktop
    ));
    assert_eq!(watch.get(), Some(&deadline));
}
