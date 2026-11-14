//! Real Registry/source transitions and actual `WindowSession` wait, with fake UI
//! commands only. This does not claim native HWND/Close qualification.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::CorrelationId;
use crate::app_session::compat_router::owned_tests::fixture;
use crate::app_session::window_registry::{CreateOutcome, WindowOwner};
use keld_wv::{AppWindowCommand, WebviewId, WindowSnapshot};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn opening(router: &PrimaryRouterHandle, caller: &PrimaryCallerContext) -> u64 {
    let mut windows = router
        .windows
        .as_ref()
        .expect("Registry")
        .lock()
        .expect("source");
    assert!(windows.registry.mark_ready());
    windows.subscribed = Some(WindowOwner::new(caller.identity, caller.attempt));
    let CreateOutcome::Build { build, .. } = windows
        .registry
        .create_for_caller(caller, CorrelationId(91), 11, 12, None, None)
        .expect("same canonical admission")
    else {
        panic!("new build")
    };
    build
}

fn built(build: u64) -> AppWindowEvent {
    AppWindowEvent::WindowBuilt {
        build,
        view: WebviewId(71),
        snapshot: WindowSnapshot {
            x: 1,
            y: 2,
            width: 11,
            height: 12,
            focused: false,
            maximized: false,
            full_screen: false,
        },
    }
}

#[test]
fn late_build_cleanup_runs_once_after_held_turn_deadline_for_retired_source() {
    fixture(|router, caller, _, _, _, _, commands| {
        let build = opening(router, caller);
        router
            .retire_generation_after_write_failure(caller)
            .expect("canonical A retirement");
        let mut held = Some(
            router
                .claim_window_publication(Instant::now() + Duration::from_secs(3), None)
                .expect("owner claim")
                .expect("actual turn"),
        );
        let (wait_send, waited) = mpsc::sync_channel(1);
        router
            .observe_window_turn_wait_for_test(wait_send)
            .expect("actual wait observer");
        let (done_send, done) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let source = scope.spawn(|| {
                let result = router.window_native_event_until(
                    built(build),
                    Instant::now() + Duration::from_millis(100),
                );
                let _ = done_send.send(result);
            });
            let entered = waited.recv_timeout(Duration::from_secs(2));
            let result = done.recv_timeout(Duration::from_secs(2));
            let cleanup = commands.recv_timeout(Duration::from_secs(2));
            // A broken unbounded wait is released before any assertion.
            drop(held.take());
            source.join().expect("actual source ended");
            entered.expect("actual held-turn wait");
            assert!(matches!(
                result,
                Ok(Ok(NativePublicationDisposition::Handled))
            ));
            assert!(matches!(
                cleanup,
                Ok(AppWindowCommand::DestroyWindow {
                    view: WebviewId(71)
                })
            ));
            assert!(
                commands.try_recv().is_err(),
                "no duplicate cleanup or unfenced Fatal"
            );
        });
        let windows = router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("actual state");
        assert!(
            windows
                .registry
                .native_publication_window(&AppWindowEvent::WindowReleased {
                    view: WebviewId(71)
                })
                .is_some(),
            "the same late view remains owned until its actual release"
        );
        drop(windows);
        assert!(router.current.lock().expect("same owner").is_none());
    });
}

#[test]
fn current_native_source_timeout_applies_fact_then_queues_exact_fatal_once() {
    fixture(|router, caller, _, _, _, _, commands| {
        let build = opening(router, caller);
        let mut held = Some(
            router
                .claim_window_publication(Instant::now() + Duration::from_secs(3), None)
                .expect("owner claim")
                .expect("actual turn"),
        );
        let (wait_send, waited) = mpsc::sync_channel(1);
        router
            .observe_window_turn_wait_for_test(wait_send)
            .expect("actual wait observer");
        let (done_send, done) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let source = scope.spawn(|| {
                let result = router.window_native_event_until(
                    built(build),
                    Instant::now() + Duration::from_millis(100),
                );
                let _ = done_send.send(result);
            });
            let entered = waited.recv_timeout(Duration::from_secs(2));
            let result = done.recv_timeout(Duration::from_secs(2));
            let fatal = commands.recv_timeout(Duration::from_secs(2));
            drop(held.take());
            source.join().expect("actual source ended");
            entered.expect("actual held-turn wait");
            assert!(matches!(
                result,
                Ok(Ok(NativePublicationDisposition::FatalQueued(_)))
            ));
            assert!(matches!(fatal, Ok(AppWindowCommand::Fatal)));
            assert!(commands.try_recv().is_err(), "one exact final-fence Fatal");
        });
        let windows = router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("actual state");
        assert!(
            windows
                .registry
                .native_publication_window(&AppWindowEvent::WindowReleased {
                    view: WebviewId(71)
                })
                .is_some(),
            "timeout did not discard the actual built-view fact"
        );
    });
}
