//! Real Windows pipe fixture plus the canonical Core transition/current owner.
//! These schedule checks prove the fence composition, not native COM delivery.
#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use std::sync::TryLockError;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

fn fixture(run: impl FnOnce(PrimaryRouterHandle)) {
    let (router, _peer, _owner, _window) =
        super::super::tests::initial_ready_router_with_window_loop();
    run(router);
}

fn caller(router: &PrimaryRouterHandle) -> PrimaryCallerContext {
    let current = router.current.lock().expect("fixture current owner");
    let active = current.as_ref().expect("fixture live generation");
    PrimaryCallerContext {
        attempt: active.attempt,
        identity: active.identity,
        reader_stop: Arc::clone(&active.reader_stop),
    }
}

#[test]
fn renderer_role_native_admission_runs_once_under_canonical_locks_and_releases_them() {
    fixture(|router| {
        let witness = router.renderer_role_witness(&caller(&router));
        let mut calls = 0;
        assert!(witness.try_admit_native(&mut || {
            calls += 1;
            assert!(matches!(
                router.shutdown.transition.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            assert!(matches!(
                router.current.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            true
        }));
        assert_eq!(calls, 1);
        assert!(router.shutdown.transition.try_lock().is_ok());
        assert!(router.current.try_lock().is_ok());
        // A native owner's refusal propagates and does not manufacture acceptance.
        assert!(!witness.try_admit_native(&mut || false));
        assert!(witness.is_live());
    });
}

#[test]
fn renderer_role_observation_without_canonical_fence_cannot_publish() {
    let stop = Arc::new(AtomicBool::new(false));
    let witness = RendererRoleWitness::new(Arc::clone(&stop));
    let clone = witness.clone();
    assert!(witness.is_live() && witness.matches(&stop) && witness.same_role(&clone));
    assert!(!witness.try_admit_native(&mut || panic!("observation invoked native effect")));
    stop.store(true, Ordering::Release);
    assert!(!witness.is_live() && !witness.matches(&stop));
    assert!(
        witness.same_role(&clone),
        "identity remains observable after retirement"
    );
}

#[test]
fn renderer_role_equal_attempt_and_live_foreign_boolean_do_not_authenticate() {
    fixture(|router| {
        let actual = caller(&router);
        let foreign = PrimaryCallerContext {
            reader_stop: Arc::new(AtomicBool::new(false)),
            ..actual.clone()
        };
        let forged = router.renderer_role_witness(&foreign);
        assert!(forged.is_live(), "a true observation alone is insufficient");
        assert!(!forged.try_admit_native(&mut || panic!("foreign reader identity was admitted")));
        let wrong_attempt = PrimaryCallerContext {
            attempt: actual.attempt + 1,
            ..actual
        };
        assert!(
            !router
                .renderer_role_witness(&wrong_attempt)
                .try_admit_native(&mut || panic!("foreign attempt was admitted"))
        );
    });
}

#[test]
fn renderer_role_retirement_first_prevents_waiting_native_admission() {
    fixture(|router| {
        let witness = router.renderer_role_witness(&caller(&router));
        let transition = router.shutdown.transition_guard();
        let mut current = router.current.lock().expect("retirement owner");
        let (started, start) = mpsc::sync_channel(0);
        let (finished, result) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            started.send(()).expect("admission contender started");
            let accepted =
                witness.try_admit_native(&mut || panic!("retired role reached native admission"));
            finished.send(accepted).expect("report admission result");
        });
        start
            .recv_timeout(Duration::from_secs(2))
            .expect("contender ready");
        router
            .retire_current_generation_locked(
                &mut current,
                1,
                "fixture retirement",
                "fixture link close",
            )
            .expect("canonical role retirement");
        drop(current);
        drop(transition);
        assert!(
            !result
                .recv_timeout(Duration::from_secs(2))
                .expect("bounded admission result")
        );
        worker.join().expect("admission contender completed");
    });
}

#[test]
fn renderer_role_admission_first_keeps_fixed_target_and_releases_core_before_completion() {
    fixture(|router| {
        let witness = router.renderer_role_witness(&caller(&router));
        let fixed_target = AtomicUsize::new(0);
        assert!(witness.try_admit_native(&mut || {
            fixed_target.store(7, Ordering::Release);
            true
        }));
        let (finished, result) = mpsc::sync_channel(1);
        let retire = router.clone();
        let worker = std::thread::spawn(move || {
            retire
                .retire_generation(1)
                .expect("canonical retirement after admission");
            finished.send(()).expect("retirement completed");
        });
        result
            .recv_timeout(Duration::from_secs(2))
            .expect("retirement is not blocked by native completion");
        worker.join().expect("retirement worker completed");
        assert_eq!(
            fixed_target.load(Ordering::Acquire),
            7,
            "accepted target is never resolved from a successor"
        );
        assert!(!witness.try_admit_native(&mut || panic!("retired role admitted a fresh post")));
        assert!(router.shutdown.transition.try_lock().is_ok());
        assert!(router.current.try_lock().is_ok());
    });
}

#[test]
fn renderer_role_missing_owner_or_session_shutdown_refuses_before_native_effect() {
    fixture(|router| {
        let witness = router.renderer_role_witness(&caller(&router));
        drop(router);
        assert!(!witness.try_admit_native(&mut || panic!("dead session reached native effect")));
    });
    fixture(|router| {
        let witness = router.renderer_role_witness(&caller(&router));
        {
            let _transition = router.shutdown.transition_guard();
            assert!(
                router
                    .shutdown
                    .reserve_guarded(super::super::SESSION_CLI_LEASE_LOST)
            );
        }
        assert!(
            !witness
                .try_admit_native(&mut || panic!("shutting-down session reached native effect"))
        );
    });
}

#[cfg(all(windows, debug_assertions))]
mod retirement {
    use super::*;
    use crate::app_session::window_registry::{RendererRoute, WindowOwner};
    use crate::app_session::window_router::WindowSession;
    use keld_wv::renderer_bridge::RouteRetirementPermit;
    use keld_wv::renderer_contract::NativeDocumentFixture;

    fn registered(
        run: impl FnOnce(
            PrimaryRouterHandle,
            PrimaryCallerContext,
            NativeDocumentFixture,
            RendererRoute,
            RouteRetirementPermit,
        ),
    ) {
        fixture(|mut router| {
            let caller = caller(&router);
            let owner = WindowOwner::new(caller.identity, caller.attempt);
            let mut windows = WindowSession::new(true);
            let (selected_stage, native, route) = windows
                .fixture_original_retirement_route(owner)
                .expect("real graph and modeled original native document");
            router.windows = Some(Arc::new(Mutex::new(windows)));
            let permit = {
                let _transition = router.shutdown.transition_guard();
                let _current = router.current.lock().expect("canonical mint cut");
                let mut windows = router
                    .windows
                    .as_ref()
                    .expect("window owner")
                    .lock()
                    .expect("registry mint cut");
                windows.fixture_bind_original_caller(&caller);
                router
                    .renderer_route_retirement(&caller, &route)
                    .expect("registered retirement operation")
            };
            run(router, caller, native, route, permit);
            drop(selected_stage);
        });
    }

    #[test]
    fn renderer_retirement_permit_allows_only_same_old_owner_once_after_role_stop() {
        registered(|router, caller, native, route, permit| {
            router
                .retire_generation(caller.attempt)
                .expect("canonical role A retirement");
            assert!(caller.reader_stop.load(Ordering::Acquire));
            assert!(
                native.document().is_current(),
                "physical finished document survives role loss"
            );
            assert!(
                !router
                    .renderer_role_witness(&caller)
                    .try_admit_native(&mut || panic!("dead A live admission"))
            );
            assert!(
                !permit.try_admit_route_unavailable(&route.document, &mut || false),
                "local refusal keeps operation available"
            );
            assert!(
                permit.try_admit_route_unavailable(&route.document, &mut || {
                    assert!(matches!(
                        router.shutdown.transition.try_lock(),
                        Err(TryLockError::WouldBlock)
                    ));
                    assert!(matches!(
                        router.current.try_lock(),
                        Err(TryLockError::WouldBlock)
                    ));
                    assert!(matches!(
                        router.windows.as_ref().expect("windows").try_lock(),
                        Err(TryLockError::WouldBlock)
                    ));
                    true
                })
            );
            assert!(
                !permit.try_admit_route_unavailable(&route.document, &mut || panic!(
                    "retirement replay"
                ))
            );
            assert!(router.shutdown.transition.try_lock().is_ok());
            assert!(router.current.try_lock().is_ok());
            assert!(router.windows.as_ref().expect("windows").try_lock().is_ok());
        });
    }

    fn successor_schedule(same_number: bool, retire_successor: bool) {
        registered(|router, original, native, route, permit| {
            router
                .retire_generation(original.attempt)
                .expect("retire actual A");
            let (spare, peer_b, spare_owner, spare_windows) =
                super::super::super::tests::initial_ready_router_with_window_loop();
            let successor = spare
                .current
                .lock()
                .expect("spare current")
                .take()
                .expect("spare generation")
                .writer;
            drop(spare);
            drop(spare_owner);
            drop(spare_windows);
            let attempt = if same_number {
                original.attempt
            } else {
                original.attempt + 1
            };
            router
                .install_generation(attempt, successor)
                .expect("actual B installation");
            let successor = caller(&router);
            assert!(successor.identity == original.identity);
            assert!(!Arc::ptr_eq(&successor.reader_stop, &original.reader_stop));
            {
                let _transition = router.shutdown.transition_guard();
                let current = router.current.lock().expect("actual successor cut");
                let active = current.as_ref().expect("actual B");
                assert!(Arc::ptr_eq(&active.reader_stop, &successor.reader_stop));
                router
                    .windows
                    .as_ref()
                    .expect("windows")
                    .lock()
                    .expect("registry")
                    .fixture_adopt_retained_original(&successor);
            }
            if retire_successor {
                router
                    .retire_generation(successor.attempt)
                    .expect("actual B retirement");
                assert!(router.current.lock().expect("post B current").is_none());
            }
            assert!(
                native.document().is_current(),
                "same physical page survives both roles"
            );
            let calls = AtomicUsize::new(0);
            let admitted = permit.try_admit_route_unavailable(&route.document, &mut || {
                calls.fetch_add(1, Ordering::AcqRel);
                true
            });
            let successor_live = router
                .renderer_role_witness(&successor)
                .try_admit_native(&mut || true);
            let mut owned = crate::app_session::PrimaryRouter {
                primary_writer_worker: None,
                handle: router.clone(),
                fs_worker: None,
            };
            owned
                .stop_and_join()
                .expect("actual spawned reader cleanup");
            drop(peer_b);
            assert_eq!(successor_live, !retire_successor);
            assert!(
                !admitted,
                "old A demoted B's retained document after adoption"
            );
            assert_eq!(calls.load(Ordering::Acquire), 0);
        });
    }

    #[test]
    fn renderer_retirement_permit_cannot_demote_live_successor_on_same_physical_document() {
        successor_schedule(false, false);
    }
    #[test]
    fn renderer_retirement_permit_refuses_same_number_live_successor_allocation() {
        successor_schedule(true, false);
    }
    #[test]
    fn renderer_retirement_permit_refuses_same_number_successor_even_after_it_retires() {
        successor_schedule(true, true);
    }

    #[test]
    fn renderer_retirement_permit_refuses_native_replacement_and_missing_owner() {
        registered(|router, _, mut native, route, permit| {
            native.revoke();
            assert!(
                !permit.try_admit_route_unavailable(&route.document, &mut || panic!(
                    "revoked native document"
                ))
            );
            drop(router);
            assert!(
                !permit.try_admit_route_unavailable(&route.document, &mut || panic!(
                    "expired Core owner"
                ))
            );
        });
    }
}
