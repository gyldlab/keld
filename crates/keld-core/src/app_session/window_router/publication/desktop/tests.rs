//! Real sole-writer pipes and exact Core state. Native DTOs here are modeled;
//! these controls do not qualify OS acquisition, `PMv2` or native display identity.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::compat_router::owned_tests::fixture;
use crate::app_session::primary_writer::ControlMessage;
use crate::app_session::{BootstrapStream, CallError, FrameKind, PrimaryCallerContext};
use keld_ipc::codec::decode;
use keld_ipc::link::read_frame;
use keld_ipc::window::{DesktopFact, DesktopFacts, Rect};
use keld_wv::{
    AppDesktopCapture, AppDesktopDisplay, AppDesktopFacts, AppDesktopRect, AppWindowEvent,
};
use std::time::Duration;

mod coordinator;
mod recovery;
mod waiting;

fn select(router: &PrimaryRouterHandle) {
    router
        .windows
        .as_ref()
        .expect("Registry")
        .lock()
        .expect("same source")
        .desktop_required = true;
}

fn observed() -> AppDesktopFacts {
    AppDesktopFacts {
        primary_id: 17,
        displays: vec![AppDesktopDisplay {
            id: 17,
            bounds: AppDesktopRect {
                x: 0,
                y: 0,
                width: 1440,
                height: 900,
            },
            work_area: AppDesktopRect {
                x: 0,
                y: 24,
                width: 1440,
                height: 876,
            },
            scale_factor: 2.0,
        }],
        should_use_dark_colors: false,
    }
}

fn expected(epoch: u64, populated: bool) -> WindowEvent {
    WindowEvent::DesktopSnapshot {
        epoch,
        snapshot: populated.then(|| DesktopFacts {
            primary_id: 17,
            displays: vec![DesktopFact {
                id: 17,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: 1440,
                    height: 900,
                },
                work_area: Rect {
                    x: 0,
                    y: 24,
                    width: 1440,
                    height: 876,
                },
                scale_factor: 2.0,
            }],
            should_use_dark_colors: false,
        }),
    }
}

fn publish(
    router: &PrimaryRouterHandle,
    capture: Option<AppDesktopCapture>,
    facts: Option<AppDesktopFacts>,
) {
    let event = match capture {
        None => AppWindowEvent::DesktopObserved { facts },
        Some(capture) => AppWindowEvent::WindowsDesktopObserved { capture, facts },
    };
    assert!(matches!(
        router
            .window_native_event(event)
            .expect("owned source publication"),
        NativePublicationDisposition::Handled
    ));
}

fn subscribe(router: &PrimaryRouterHandle, caller: &PrimaryCallerContext, correlation: u32) {
    router
        .subscribe_window(
            caller,
            CorrelationId(correlation),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
        .expect("actual subscription source");
}

fn fact(peer: &mut BootstrapStream, event: &WindowEvent) {
    let (header, body) = read_frame(peer).expect("actual Window event");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0)
        ),
    );
    assert_eq!(
        decode::<WindowEvent>(&body).expect("canonical Window codec"),
        *event
    );
}

fn subscribed(peer: &mut BootstrapStream, correlation: u32) {
    let (header, body) = read_frame(peer).expect("actual accepted reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(correlation)
        ),
    );
    assert_eq!(
        decode::<keld_ipc::LifecycleResponse>(&body).expect("canonical lifecycle codec"),
        keld_ipc::LifecycleResponse::Subscribed,
    );
}

fn refusal(peer: &mut BootstrapStream, correlation: u32, code: &str) {
    let (header, body) = read_frame(peer).expect("actual correlated refusal");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Err,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(correlation)
        ),
    );
    assert_eq!(
        decode::<CallError>(&body)
            .expect("canonical error codec")
            .code,
        code
    );
}

fn ping(router: &PrimaryRouterHandle, caller: &PrimaryCallerContext, peer: &mut BootstrapStream) {
    router
        .submit_primary_control(
            caller,
            ControlMessage::Ping {
                channel: keld_ipc::LIFECYCLE_CHANNEL,
                correlation: CorrelationId(99),
            },
            Instant::now() + Duration::from_secs(2),
            "desktop test next physical frame",
        )
        .expect("same writer Ping");
    let (header, bytes) = read_frame(peer).expect("next frame must be Ping");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Ping, CorrelationId(99))
    );
    assert_eq!(bytes, [] as [u8; 0]);
}

#[test]
fn pending_keeps_exact_link_clock_then_snapshot_precedes_subscribed() {
    fixture(|router, caller, target, peer, _, _, _| {
        select(router);
        let deadline = Instant::now() + Duration::from_secs(2);
        router
            .subscribe_window_until(
                caller,
                CorrelationId(71),
                keld_ipc::channel_table::WINDOW.wire_id(),
                deadline,
            )
            .expect("early source");
        {
            let session = router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("source");
            let pending = session
                .pending_desktop
                .as_ref()
                .expect("one pending source");
            assert!(Arc::ptr_eq(&pending.target, target));
            assert_eq!(pending.deadline, deadline);
            assert!(session.subscribed.is_none());
            assert!(session.publication.replay.is_none());
        }
        subscribe(router, caller, 70);
        refusal(peer, 70, "KELD-IPC-005");
        publish(router, None, Some(observed()));
        fact(peer, &expected(1, true));
        subscribed(peer, 71);
        subscribe(router, caller, 72);
        subscribed(peer, 72);
        ping(router, caller, peer);
    });
}

#[test]
fn pending_same_correlation_is_protocol_failure_and_retirement_cannot_settle_it_later() {
    fixture(|router, caller, _, peer, _, _, commands| {
        select(router);
        subscribe(router, caller, 51);
        let error = router
            .subscribe_window(
                caller,
                CorrelationId(51),
                keld_ipc::channel_table::WINDOW.wire_id(),
            )
            .expect_err("duplicate pending correlation");
        assert!(error.to_string().contains("KELD-IPC-005"));
        // The exact original reader normally forwards this error to its owner.
        // Exercise that owner's canonical revocation, not a fake second reply.
        router
            .retire_generation(1)
            .expect("same canonical retirement");
        publish(
            router,
            Some(AppDesktopCapture::from_current(
                caller.attempt,
                Arc::clone(&caller.reader_stop),
            )),
            Some(observed()),
        );
        let session = router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("source");
        assert!(session.pending_desktop.is_none());
        assert!(session.subscribed.is_none());
        assert!(session.desktop_latest.is_none());
        drop(session);
        assert!(commands.try_recv().is_err());
        assert!(
            read_frame(peer).is_err(),
            "no second correlated settlement before actual EOF"
        );
    });
}

#[test]
fn unavailable_current_capture_sends_only_049_and_no_adoption_or_subscription() {
    fixture(|router, caller, _, peer, _, _, _| {
        select(router);
        subscribe(router, caller, 91);
        publish(router, None, None);
        refusal(peer, 91, "KELD-CORE-049");
        subscribe(router, caller, 92);
        refusal(peer, 92, "KELD-CORE-049");
        {
            let session = router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("source");
            assert!(session.subscribed.is_none());
            assert!(session.pending_desktop.is_none());
            assert_eq!(session.desktop_epoch.0, 1);
        }
        ping(router, caller, peer);
    });
}

#[test]
fn current_equal_sample_is_idempotent_and_invalidation_is_an_ordered_fact() {
    fixture(|router, caller, _, peer, _, _, _| {
        select(router);
        publish(router, None, Some(observed()));
        subscribe(router, caller, 81);
        fact(peer, &expected(1, true));
        subscribed(peer, 81);
        let capture =
            AppDesktopCapture::from_current(caller.attempt, Arc::clone(&caller.reader_stop));
        publish(router, Some(capture.clone()), Some(observed()));
        ping(router, caller, peer);
        publish(router, Some(capture), None);
        fact(peer, &expected(2, false));
        ping(router, caller, peer);
    });
}
