use super::*;
use keld_ipc::window::{WindowRef, WindowRequest, WindowResponse};
use keld_wv::{AppWindowCommand, WebviewId, WindowSnapshot};
use std::sync::atomic::Ordering;

/// Keep the real installed reader under an exact cleanup guard even on assertion
/// unwind. This test owner is not a production recovery or native proof.
struct Successor {
    router: PrimaryRouterHandle,
    target: Arc<WriterLink>,
    live: bool,
}

impl Successor {
    fn install(router: &PrimaryRouterHandle) -> (Self, BootstrapStream) {
        let (spare, peer, owner, windows) =
            crate::app_session::tests::initial_ready_router_with_window_loop();
        let stream = spare
            .current
            .lock()
            .expect("fixture current")
            .take()
            .expect("actual spare pipe")
            .writer;
        drop(spare);
        drop(owner);
        drop(windows);
        router
            .install_generation(1, stream)
            .expect("actual same-number successor");
        let target = router
            .current_primary_writer_target()
            .expect("current target")
            .expect("actual successor link");
        (
            Self {
                router: router.clone(),
                target,
                live: true,
            },
            peer,
        )
    }

    fn close(mut self) {
        self.router
            .retire_generation_after_write_failure(self.target.caller())
            .expect("exact fixture cleanup, no fabricated failure report");
        self.router
            .readers
            .lock()
            .expect("reader owner")
            .remove(&1)
            .expect("actual installed reader")
            .join()
            .expect("actual reader thread joined")
            .expect("stopped reader");
        self.live = false;
    }
}

impl Drop for Successor {
    fn drop(&mut self) {
        if self.live {
            let _ = self
                .router
                .retire_generation_after_write_failure(self.target.caller());
            let reader = self
                .router
                .readers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&1);
            if let Some(reader) = reader {
                let _ = reader.join();
            }
        }
    }
}

fn build_survivor(
    router: &PrimaryRouterHandle,
    caller: &PrimaryCallerContext,
    peer: &mut BootstrapStream,
    commands: &std::sync::mpsc::Receiver<AppWindowCommand>,
) -> WindowRef {
    {
        let _transition = router.shutdown.transition_guard();
        assert!(
            router
                .mark_window_registry_ready_guarded()
                .expect("existing Registry gate")
        );
        // Model the already-initialized Host in this source-only fixture. The
        // canonical request gate is distinct from the Registry's ready bit.
        router.window_ready.store(true, Ordering::Release);
    }
    let bytes = keld_ipc::codec::encode(&WindowRequest::Create {
        width: 640,
        height: 480,
        x: None,
        y: None,
    })
    .expect("canonical request");
    router
        .dispatch_window(caller, CorrelationId(102), &bytes)
        .expect("actual source admission");
    let AppWindowCommand::CreateWindow { build, .. } = commands
        .recv_timeout(Duration::from_secs(2))
        .expect("actual create command")
    else {
        panic!("existing Window create owner required");
    };
    let snapshot = WindowSnapshot {
        x: 0,
        y: 0,
        width: 640,
        height: 480,
        focused: false,
        maximized: false,
        full_screen: false,
    };
    assert!(matches!(
        router
            .window_native_event(AppWindowEvent::WindowBuilt {
                build,
                view: WebviewId(7),
                snapshot,
            })
            .expect("modeled native fact through actual Registry"),
        NativePublicationDisposition::Handled,
    ));
    let (header, bytes) = read_frame(peer).expect("actual Created");
    assert_eq!(header.kind, FrameKind::Event);
    let WindowEvent::Created { window, .. } =
        decode::<WindowEvent>(&bytes).expect("canonical Created")
    else {
        panic!("first Window build fact must be Created");
    };
    let (header, bytes) = read_frame(peer).expect("actual build reply");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(102))
    );
    assert_eq!(
        decode::<WindowResponse>(&bytes).expect("canonical Window reply"),
        WindowResponse::Created { window },
    );
    window
}

fn retained_ready(peer: &mut BootstrapStream) {
    let (header, bytes) = read_frame(peer).expect("actual retained Ready");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Event,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(0)
        )
    );
    assert_eq!(
        decode::<keld_ipc::LifecycleEvent>(&bytes).expect("canonical retained lifecycle event"),
        keld_ipc::LifecycleEvent::Ready
    );
}

#[test]
fn replacement_resamples_before_survivor_replay_and_late_same_number_a_is_inert() {
    fixture(|router, caller, _, peer, _, _, commands| {
        select(router);
        publish(router, None, Some(observed()));
        subscribe(router, caller, 101);
        fact(peer, &expected(1, true));
        subscribed(peer, 101);
        let survivor = build_survivor(router, caller, peer, commands);
        router.retire_generation(1).expect("actual A retirement");
        {
            let session = router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("source");
            assert!(session.initial_desktop_closed);
            assert!(!session.desktop_seen);
            assert!(session.desktop_latest.is_none());
            assert_eq!(session.desktop_epoch.0, 1);
        }
        let (b, mut peer_b) = Successor::install(router);
        retained_ready(&mut peer_b);
        assert_eq!(b.target.caller().attempt, caller.attempt);
        assert!(!Arc::ptr_eq(
            &b.target.caller().reader_stop,
            &caller.reader_stop
        ));
        let AppWindowCommand::WindowsDesktopProducer { capture: capture_b } = commands
            .recv_timeout(Duration::from_secs(2))
            .expect("actual producer binding")
        else {
            panic!("fresh capture must bind before any replacement publication");
        };
        assert!(capture_b.matches(1, &b.target.caller().reader_stop));
        subscribe(router, b.target.caller(), 103);
        let capture_a =
            AppDesktopCapture::from_current(caller.attempt, Arc::clone(&caller.reader_stop));
        let mut changed = observed();
        changed.displays[0].work_area.height -= 1;
        for old_fact in [None, Some(observed()), Some(changed)] {
            publish(router, Some(capture_a.clone()), old_fact);
            let session = router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("source");
            assert!(!session.desktop_seen);
            assert!(session.desktop_latest.is_none());
            assert_eq!(session.desktop_epoch.0, 1);
            assert_eq!(
                session
                    .pending_desktop
                    .as_ref()
                    .expect("same B pending")
                    .correlation,
                CorrelationId(103)
            );
        }
        // Equal numerical facts for B are still its first fresh observation.
        publish(router, Some(capture_b.clone()), Some(observed()));
        fact(&mut peer_b, &expected(2, true));
        let (header, bytes) = read_frame(&mut peer_b).expect("actual survivor replay");
        assert_eq!(header.kind, FrameKind::Event);
        assert!(matches!(
            decode::<WindowEvent>(&bytes).expect("canonical survivor"),
            WindowEvent::Created { window, .. } if window == survivor
        ));
        subscribed(&mut peer_b, 103);
        publish(router, Some(capture_b), Some(observed()));
        ping(router, b.target.caller(), &mut peer_b);
        assert!(!b.target.caller().reader_stop.load(Ordering::Acquire));
        b.close();
        // A remains ineligible even after B has gone; epoch/history persist.
        publish(router, Some(capture_a), None);
        let session = router
            .windows
            .as_ref()
            .expect("Registry")
            .lock()
            .expect("source");
        assert_eq!(session.desktop_epoch.0, 2);
        assert!(!session.desktop_seen);
        assert!(session.pending_desktop.is_none());
        assert!(session.desktop_latest.is_none());
    });
}

#[test]
fn unavailable_replacement_never_uses_retired_eligible_cache() {
    fixture(|router, caller, _, peer, _, _, commands| {
        select(router);
        publish(router, None, Some(observed()));
        subscribe(router, caller, 111);
        fact(peer, &expected(1, true));
        subscribed(peer, 111);
        router.retire_generation(1).expect("A retirement");
        let (b, mut peer_b) = Successor::install(router);
        let AppWindowCommand::WindowsDesktopProducer { capture } = commands
            .recv_timeout(Duration::from_secs(2))
            .expect("fresh B producer")
        else {
            panic!("actual B producer required");
        };
        subscribe(router, b.target.caller(), 112);
        publish(router, Some(capture), None);
        refusal(&mut peer_b, 112, "KELD-CORE-049");
        ping(router, b.target.caller(), &mut peer_b);
        {
            let session = router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("source");
            assert_eq!(session.desktop_epoch.0, 2);
            assert!(session.subscribed.is_none());
        }
        b.close();
    });
}
