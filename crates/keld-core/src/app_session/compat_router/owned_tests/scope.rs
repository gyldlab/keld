//! Actual captured Entry and same ledger; native APIs are not modeled as success.
use super::*;
use crate::app_session::window_registry::{RendererRoute, WindowOwner};
use keld_wv::renderer_bridge::RendererRoleWitness;

fn scoped(
    run: impl FnOnce(
        &PrimaryRouterHandle,
        &PrimaryCallerContext,
        &RendererRoute,
        &RendererRoleWitness,
        &Receiver<RendererBridgeOutcome>,
    ),
) {
    fixture(|base, caller, _, _, _, outcomes, _| {
        let mut router = base.clone();
        let mut windows = WindowSession::new(true);
        let (root, native, route) = windows
            .fixture_original_retirement_route(WindowOwner::new(caller.identity, caller.attempt))
            .expect("real retained source fixture");
        {
            let _transition = router.shutdown.transition_guard();
            let _current = router.current.lock().expect("actual binding cut");
            windows.fixture_bind_original_caller(caller);
        }
        router.windows = Some(Arc::new(Mutex::new(windows)));
        let scope = router.renderer_role_witness_for_window(caller, route.window);
        run(&router, caller, &route, &scope, outcomes);
        drop(native);
        drop(root);
    });
}

#[test]
fn scope_close_is_monotonic_and_waits_for_actual_queued_old_control_storage() {
    scoped(|router, caller, route, scope, _| {
        assert!(scope.has_native_view_scope());
        assert!(
            !scope.native_storage_is_quiescent(),
            "open producer is not drained"
        );
        let token = scope
            .try_reserve_native_control(1, 1025)
            .expect("same scoped native record");
        assert!(scope.close_native_offers());
        assert!(scope.close_native_offers(), "same close is idempotent");
        assert!(!scope.try_admit_native(&mut || panic!("closed producer reached native entry")));
        assert!(scope.try_reserve_native_control(1, 1025).is_none());
        assert!(
            !scope.native_storage_is_quiescent(),
            "queued token has not reached Wv but remains owned"
        );
        {
            let _transition = router.shutdown.transition_guard();
            let _current = router
                .current
                .lock()
                .expect("canonical adoption fixture cut");
            let mut windows = router
                .windows
                .as_ref()
                .expect("same Registry")
                .lock()
                .expect("entry");
            windows.fixture_adopt_retained_original(caller);
            assert_eq!(windows.native_view_scope_status(route.window), Some(true));
        }
        assert!(!scope.native_storage_is_quiescent());
        drop(token);
        assert!(
            scope.native_storage_is_quiescent(),
            "actual token release, no WindowReleased dependency"
        );
    });
}

#[test]
fn same_physical_scope_survives_actual_same_number_successor_and_its_retirement() {
    scoped(|router, original, route, scope, outcomes| {
        let token = scope
            .try_reserve_native_control(1, 1025)
            .expect("A queued native storage");
        router
            .retire_generation(original.attempt)
            .expect("actual A retirement");
        let (spare, peer_b, spare_owner, spare_windows) =
            crate::app_session::tests::initial_ready_router_with_window_loop();
        let stream = spare
            .current
            .lock()
            .expect("spare")
            .take()
            .expect("actual B stream")
            .writer;
        drop(spare);
        drop(spare_owner);
        drop(spare_windows);
        router
            .install_generation(original.attempt, stream)
            .expect("actual equal-number B installation");
        let successor = {
            let current = router.current.lock().expect("actual B");
            let active = current.as_ref().expect("installed B");
            PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            }
        };
        assert!(!Arc::ptr_eq(&original.reader_stop, &successor.reader_stop));
        {
            let _transition = router.shutdown.transition_guard();
            let _current = router.current.lock().expect("actual B adoption");
            router
                .windows
                .as_ref()
                .expect("same Registry")
                .lock()
                .expect("same Entry")
                .fixture_adopt_retained_original(&successor);
        }
        router
            .retire_generation(successor.attempt)
            .expect("actual B retirement");
        assert!(
            router
                .current
                .lock()
                .expect("no current successor")
                .is_none()
        );
        assert!(
            scope.has_native_view_scope(),
            "lifetime is not A's role grant"
        );
        assert!(
            scope.close_native_offers(),
            "actual original View may close after both roles"
        );
        while let Ok(outcome) = outcomes.try_recv() {
            drop(outcome);
        } // actual queued backing disposal
        assert!(
            !scope.native_storage_is_quiescent(),
            "A's token is still retained"
        );
        assert_eq!(
            router
                .windows
                .as_ref()
                .expect("same Entry")
                .lock()
                .expect("Registry")
                .native_view_scope_status(route.window),
            Some(true)
        );
        drop(token);
        assert!(scope.native_storage_is_quiescent());
        let mut owned = crate::app_session::PrimaryRouter {
            primary_writer_worker: None,
            handle: router.clone(),
            fs_worker: None,
        };
        owned
            .stop_and_join()
            .expect("actual spawned reader owners joined");
        drop(peer_b);
    });
}

#[test]
fn missing_foreign_and_unassociated_scope_never_grant_storage_drain() {
    scoped(|router, caller, route, scope, _| {
        let unscoped = router.renderer_role_witness(caller);
        assert!(!unscoped.has_native_view_scope());
        assert!(!unscoped.close_native_offers());
        assert!(!unscoped.native_storage_is_quiescent());
        assert!(unscoped.try_reserve_native_control(1, 1025).is_none());
        let mut foreign = route.window;
        foreign.incarnation.0 = foreign
            .incarnation
            .0
            .checked_add(1)
            .expect("fixture incarnation");
        let foreign = router.renderer_role_witness_for_window(caller, foreign);
        assert!(!foreign.has_native_view_scope());
        assert!(!foreign.close_native_offers());
        let owner = router.compat.as_ref().expect("same ledger");
        let unmatched = owner
            .lock()
            .expect("explicit old unassociated test record")
            .reserve_native_control(caller, 1, 1025, Arc::clone(owner))
            .expect("actual token");
        assert!(scope.close_native_offers());
        assert!(
            !scope.native_storage_is_quiescent(),
            "unknown live native association cannot be ignored"
        );
        drop(unmatched);
        assert!(scope.native_storage_is_quiescent());
    });
}
