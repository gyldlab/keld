//! Actual Windows pipe/writer controls. Fixture identities and modeled Registry
//! state do not qualify Runtime admission, native registration, or native UI.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::primary_writer::{PrimaryWriter, PrimaryWriterWorker, WriterLink};
use crate::app_session::window_router::WindowSession;
use keld_ipc::AppLinkDeadlines as _;
use keld_ipc::codec::decode;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// Install the real sole writer on the existing fixture's exact pipe. No
/// principal, role policy, or native completion is synthesized by this helper.
pub(in crate::app_session) fn install_writer(
    router: &mut PrimaryRouterHandle,
) -> PrimaryWriterWorker {
    {
        let mut current = router.current.lock().expect("same active fixture");
        let active = current.as_mut().expect("original pipe");
        active.reader_stop = router.shutdown.register_generation_reader();
        let caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        active.writer_link = Some(
            WriterLink::new(active.writer.try_clone().expect("same actual pipe"), caller)
                .expect("exact original link"),
        );
    }
    let (writer, worker) =
        PrimaryWriterWorker::start(router.clone()).expect("single physical writer");
    if let Some(owner) = &router.compat {
        owner
            .lock()
            .expect("existing Compat")
            .bind_primary_writer(writer.clone())
            .expect("same progress owner");
    }
    router.primary_writer = Some(writer);
    worker
}

pub(in crate::app_session) fn fixture(
    run: impl FnOnce(
        &PrimaryRouterHandle,
        &PrimaryCallerContext,
        &Arc<WriterLink>,
        &mut keld_ipc::BootstrapStream,
        &PrimaryWriter,
        &Receiver<RendererBridgeOutcome>,
        &Receiver<keld_wv::AppWindowCommand>,
    ),
) {
    let (mut router, mut peer, _owner, windows) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    router.windows = Some(Arc::new(Mutex::new(WindowSession::new(true))));
    let caller;
    let target;
    {
        let mut current = router.current.lock().expect("actual generation");
        let active = current.as_mut().expect("actual pipe");
        active.reader_stop = router.shutdown.register_generation_reader();
        caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        target = WriterLink::new(
            active.writer.try_clone().expect("same pipe"),
            caller.clone(),
        )
        .expect("exact captured target");
        active.writer_link = Some(Arc::clone(&target));
    }
    let (writer, mut worker) = PrimaryWriterWorker::start(router.clone()).expect("sole writer");
    router.primary_writer = Some(writer.clone());
    let (mut session, outcomes) = super::test_fixture::availability_session();
    session
        .bind_primary_writer(writer.clone())
        .expect("same ledger progress");
    router.compat = Some(Arc::new(Mutex::new(session)));
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("original bounded pipe");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(
            &router, &caller, &target, &mut peer, &writer, &outcomes, &windows,
        );
    }));
    let cleanup = worker.stop_and_join();
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
    cleanup.expect("same worker joined outside all canonical guards");
}

#[test]
fn owned_window_and_compat_subscriptions_use_the_same_physical_writer() {
    fixture(|router, caller, _, peer, _, outcomes, _| {
        router
            .subscribe_window(
                caller,
                CorrelationId(11),
                keld_ipc::channel_table::WINDOW.wire_id(),
            )
            .expect("actual Window publication");
        let (header, bytes) = keld_ipc::link::read_frame(peer).expect("actual Window reply");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Reply, CorrelationId(11))
        );
        assert_eq!(
            decode::<keld_ipc::LifecycleResponse>(&bytes).expect("canonical reply"),
            keld_ipc::LifecycleResponse::Subscribed
        );
        router
            .subscribe_compat(
                caller,
                CorrelationId(12),
                keld_ipc::channel_table::COMPAT_CONTROL.wire_id(),
            )
            .expect("actual Compat publication");
        let (header, bytes) = keld_ipc::link::read_frame(peer).expect("actual Compat reply");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Reply, CorrelationId(12))
        );
        assert_eq!(
            decode::<keld_ipc::LifecycleResponse>(&bytes).expect("canonical reply"),
            keld_ipc::LifecycleResponse::Subscribed
        );
        assert!(
            router
                .compat
                .as_ref()
                .expect("same owner")
                .lock()
                .expect("ledger")
                .subscribed_to(caller)
        );
        assert!(
            outcomes.try_recv().is_err(),
            "no document means no fabricated native route"
        );
    });
}

#[test]
fn owned_compat_subscription_waits_for_one_real_control_position_outside_fences() {
    fixture(|router, caller, _, peer, writer, _, _| {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut occupied: Vec<_> = (0..4)
            .map(|_| {
                writer
                    .try_reserve_control(deadline)
                    .expect("four actual CONTROL positions")
            })
            .collect();
        let (at_wait, waiting) = mpsc::sync_channel(1);
        writer.observe_next_capacity_wait_for_test(at_wait);
        let source = router.clone();
        let captured = caller.clone();
        let thread = std::thread::spawn(move || {
            source.subscribe_compat(
                &captured,
                CorrelationId(31),
                keld_ipc::channel_table::COMPAT_CONTROL.wire_id(),
            )
        });
        let waited = waiting.recv_timeout(Duration::from_secs(2));
        let unlocked = router.shutdown.transition.try_lock().is_ok()
            && router.current.try_lock().is_ok()
            && router
                .windows
                .as_ref()
                .expect("Registry")
                .try_lock()
                .is_ok()
            && router.compat.as_ref().expect("Compat").try_lock().is_ok();
        drop(occupied.pop()); // cleanup always releases the actual wait before assertions
        let result = thread.join().expect("bounded source joined");
        drop(occupied);
        assert!(
            waited.is_ok(),
            "the real capacity Condvar was never reached"
        );
        assert!(unlocked, "a canonical guard crossed the capacity wait");
        result.expect("one actual released position advances publication");
        let (header, bytes) = keld_ipc::link::read_frame(peer).expect("actual subscribed frame");
        assert_eq!(header.corr, CorrelationId(31));
        assert_eq!(
            decode::<keld_ipc::LifecycleResponse>(&bytes).expect("wire schema"),
            keld_ipc::LifecycleResponse::Subscribed
        );
    });
}

#[test]
fn windows_publication_entrypoints_refuse_a_missing_writer_even_in_tests() {
    let (mut router, _peer, _owner, _windows) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let caller = {
        let current = router.current.lock().expect("fixture caller");
        let active = current.as_ref().expect("role");
        PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let (session, _outcomes) = super::test_fixture::availability_session();
    router.compat = Some(Arc::new(Mutex::new(session)));
    router.windows = Some(Arc::new(Mutex::new(WindowSession::new(true))));
    assert!(
        router
            .subscribe_window(
                &caller,
                CorrelationId(1),
                keld_ipc::channel_table::WINDOW.wire_id()
            )
            .is_err()
    );
    assert!(
        router
            .subscribe_compat(
                &caller,
                CorrelationId(2),
                keld_ipc::channel_table::COMPAT_CONTROL.wire_id()
            )
            .is_err()
    );
    assert!(
        router
            .dispatch_window(&caller, CorrelationId(3), &[])
            .is_err()
    );
    assert!(
        router
            .window_native_event(keld_wv::AppWindowEvent::HostWriterProgress)
            .is_err()
    );
    assert!(
        !router
            .compat
            .as_ref()
            .expect("same owner")
            .lock()
            .expect("ledger")
            .subscribed_to(&caller)
    );
}

#[cfg(debug_assertions)]
mod scope;
