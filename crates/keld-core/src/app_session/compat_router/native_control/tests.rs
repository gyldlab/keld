//! Same physical CONTROL slots, both independent release owners and growth.
#![allow(clippy::expect_used)]

use super::*;

mod progress;

fn caller() -> PrimaryCallerContext {
    PrimaryCallerContext {
        attempt: 1,
        identity: crate::app_session::PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn same_four_slots_include_both_hops_and_retired_native_storage() {
    let caller = caller();
    let mut ledger = CompatLedger::new();
    ledger
        .reserve_control(&caller, 1, MAX_CONTROL_BYTES)
        .expect("existing control");
    let mut leases = Vec::new();
    for _ in 0..3 {
        leases.push(
            ledger
                .reserve_native_control(&caller, 4096, 13_314, None)
                .expect("same reserved native slot"),
        );
    }
    assert!(
        ledger
            .reserve_native_control(&caller, 1, 1025, None)
            .is_none()
    );
    assert_eq!(ledger.controls_in_flight(), 4);
    caller.reader_stop.store(true, Ordering::Release);
    ledger.role_retired(&caller);
    assert!(
        ledger
            .reserve_native_control(&caller, 1, 1025, None)
            .is_none(),
        "retirement cannot replenish capacity"
    );
    drop(leases.pop());
    let replacement = ledger
        .reserve_native_control(&caller, 4096, 13_314, None)
        .expect("only actual native lease drop frees its slot");
    assert_eq!(ledger.controls_in_flight(), 4);
    drop(replacement);
    drop(leases);
    ledger.release_control(&caller, 1);
    ledger.reap_native_controls();
    assert_eq!(ledger.controls_in_flight(), 0);
}

#[test]
fn encoded_limit_and_all_representation_aggregate_are_independent() {
    let caller = caller();
    let mut ledger = CompatLedger::new();
    assert!(
        ledger
            .reserve_native_control(&caller, 4097, 20_000, None)
            .is_none()
    );
    assert!(
        ledger
            .reserve_native_control(&caller, 4096, 5119, None)
            .is_none()
    );
    let first = ledger
        .reserve_native_control(&caller, 4096, 60_000, None)
        .expect("one accounted operation");
    assert!(
        ledger
            .reserve_native_control(&caller, 4096, 5537, None)
            .is_none()
    );
    let second = ledger
        .reserve_native_control(&caller, 4096, 5536, None)
        .expect("exact existing aggregate boundary");
    assert_eq!(ledger.control_accounted_bytes(), CONTROL_ACCOUNTED_BYTES);
    drop(first);
    drop(second);
}

#[test]
fn existing_correlation_keeps_both_app_and_native_completion_halves() {
    for native_first in [false, true] {
        let caller = caller();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve_control(&caller, 9, MAX_CONTROL_BYTES)
            .expect("app correlation");
        let lease = ledger
            .lease_existing_control(&caller, 9, 0, None)
            .expect("same slot lease");
        assert_eq!(
            ledger.controls_in_flight(),
            1,
            "native hop never consumes a second slot"
        );
        assert_eq!(
            ledger.control_accounted_bytes(),
            MAX_CONTROL_BYTES + CONTROL_RECORD_BYTES
        );
        assert!(
            ledger.lease_existing_control(&caller, 9, 0, None).is_none(),
            "duplicate custody cannot be minted"
        );
        if native_first {
            drop(lease);
            ledger.reap_native_controls();
            assert_eq!(
                ledger.controls_in_flight(),
                1,
                "native completion is not app-link completion"
            );
            ledger.release_control(&caller, 9);
        } else {
            ledger.release_control(&caller, 9);
            assert_eq!(
                ledger.controls_in_flight(),
                1,
                "app-link completion is not native completion"
            );
            drop(lease);
            ledger.reap_native_controls();
        }
        assert_eq!(ledger.controls_in_flight(), 0);
    }
}

#[test]
fn additive_growth_preserves_simultaneous_original_storage_and_failed_charge() {
    let caller = caller();
    let mut ledger = CompatLedger::new();
    ledger
        .reserve_control(&caller, 7, MAX_CONTROL_BYTES)
        .expect("source control");
    let lease = ledger
        .lease_existing_control(&caller, 7, 17, None)
        .expect("same source storage");
    let released = Arc::clone(
        ledger.controls[0]
            .as_ref()
            .expect("exact slot")
            .native_release
            .as_ref()
            .expect("native release"),
    );
    let original = ledger.control_accounted_bytes();
    assert_eq!(original, MAX_CONTROL_BYTES + CONTROL_RECORD_BYTES + 17);
    assert!(ledger.extend_native_storage(&released, 3000, 9002));
    assert_eq!(
        ledger.control_accounted_bytes(),
        original + 9002,
        "JSON/UTF16 adds to retained DTO/source bytes"
    );
    let grown = ledger.control_accounted_bytes();
    assert!(!ledger.extend_native_storage(&released, 4097, 10));
    assert!(!ledger.extend_native_storage(&released, 1, usize::MAX));
    assert!(!ledger.extend_native_storage(&released, 1, CONTROL_ACCOUNTED_BYTES));
    assert_eq!(
        ledger.control_accounted_bytes(),
        grown,
        "every failed growth retains the previous charge"
    );
    assert!(ledger.extend_native_storage(&released, 4096, CONTROL_ACCOUNTED_BYTES - grown));
    assert_eq!(ledger.control_accounted_bytes(), CONTROL_ACCOUNTED_BYTES);
    assert!(!ledger.extend_native_storage(&released, 4096, 1));
    assert_eq!(ledger.controls_in_flight(), 1);
    ledger.release_control(&caller, 7);
    drop(lease);
    ledger.reap_native_controls();
    assert_eq!(ledger.controls_in_flight(), 0);
}

#[test]
fn equal_correlation_and_attempt_do_not_transfer_another_callers_control() {
    let caller = caller();
    let foreign = PrimaryCallerContext {
        reader_stop: Arc::new(AtomicBool::new(false)),
        ..caller.clone()
    };
    let mut ledger = CompatLedger::new();
    ledger
        .reserve_control(&caller, 4, MAX_CONTROL_BYTES)
        .expect("original control");
    assert!(
        ledger
            .lease_existing_control(&foreign, 4, 0, None)
            .is_none()
    );
    ledger.release_control(&foreign, 4);
    assert_eq!(ledger.controls_in_flight(), 1);
    let original = ledger
        .lease_existing_control(&caller, 4, 0, None)
        .expect("original exact caller");
    caller.reader_stop.store(true, Ordering::Release);
    ledger.role_retired(&caller);
    ledger.release_control(&caller, 4);
    assert_eq!(
        ledger.controls_in_flight(),
        1,
        "role retirement cannot release retained native storage"
    );
    drop(original);
    ledger.reap_native_controls();
    assert_eq!(ledger.controls_in_flight(), 0);
}

#[test]
fn opaque_native_token_adds_storage_on_the_actual_retained_session() {
    let caller = caller();
    let (mut session, _outcomes) = super::super::test_fixture::availability_session();
    let (progress, _progress_receiver) = std::sync::mpsc::channel();
    session
        .bind_primary_writer(crate::app_session::primary_writer::PrimaryWriter::new(
            progress,
        ))
        .expect("bind actual shared wake owner");
    let owner = Arc::new(Mutex::new(session));
    let lease = owner
        .lock()
        .expect("same session")
        .reserve_native_control(&caller, 1, 2048, Arc::clone(&owner))
        .expect("actual native lease");
    assert!(lease.try_extend_storage(1024, 4096));
    assert_eq!(
        owner
            .lock()
            .expect("same ledger")
            .ledger
            .control_accounted_bytes(),
        6144
    );
    assert!(!lease.try_extend_storage(4097, 1));
    assert!(!lease.try_extend_storage(1024, usize::MAX));
    assert_eq!(
        owner
            .lock()
            .expect("old charge retained")
            .ledger
            .control_accounted_bytes(),
        6144
    );
    drop(lease);
    let mut state = owner.lock().expect("native completion");
    state.ledger.reap_native_controls();
    assert_eq!(state.ledger.controls_in_flight(), 0);
}

#[test]
fn unscoped_and_foreign_roles_cannot_reserve_native_control() {
    super::super::owned_tests::fixture(|router, actual, _, _, _, _, _| {
        let foreign = PrimaryCallerContext {
            reader_stop: Arc::new(AtomicBool::new(false)),
            ..actual.clone()
        };
        for caller in [&foreign, actual] {
            assert!(
                router
                    .renderer_role_witness(caller)
                    .try_reserve_native_control(1, 1025)
                    .is_none(),
                "a current role alone cannot invent an Original View scope"
            );
        }
        assert_eq!(
            router
                .compat
                .as_ref()
                .expect("same owner")
                .lock()
                .expect("same ledger")
                .ledger
                .controls_in_flight(),
            0
        );
    });
}

#[cfg(debug_assertions)]
#[test]
fn only_actual_live_role_can_reserve_but_retirement_cannot_drop_native_custody() {
    let (mut router, _peer, _guardian, _windows) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (mut session, _outcomes) = super::super::test_fixture::availability_session();
    let (progress, _progress_receiver) = std::sync::mpsc::channel();
    session
        .bind_primary_writer(crate::app_session::primary_writer::PrimaryWriter::new(
            progress,
        ))
        .expect("bind actual shared wake owner");
    let owner = Arc::new(Mutex::new(session));
    router.compat = Some(Arc::clone(&owner));
    let actual = {
        let current = router.current.lock().expect("actual owner");
        let active = current.as_ref().expect("active original link");
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let mut windows = crate::app_session::window_router::WindowSession::new(true);
    let (_source, _native, route) = windows
        .fixture_original_retirement_route(crate::app_session::window_registry::WindowOwner::new(
            actual.identity,
            actual.attempt,
        ))
        .expect("same admitted Original source and actual Registry Entry");
    {
        let _transition = router.shutdown.transition_guard();
        let _current = router.current.lock().expect("canonical binding cut");
        windows.fixture_bind_original_caller(&actual);
    }
    router.windows = Some(Arc::new(Mutex::new(windows)));
    let foreign = PrimaryCallerContext {
        reader_stop: Arc::new(AtomicBool::new(false)),
        ..actual.clone()
    };
    assert!(
        router
            .renderer_role_witness_for_window(&foreign, route.window)
            .try_reserve_native_control(1, 1025)
            .is_none()
    );
    let witness = router.renderer_role_witness_for_window(&actual, route.window);
    assert!(witness.has_native_view_scope());
    let lease = witness
        .try_reserve_native_control(1, 1025)
        .expect("canonical owner reserves");
    router
        .retire_generation_after_write_failure(&actual)
        .expect("actual canonical retirement");
    assert!(witness.try_reserve_native_control(1, 1025).is_none());
    assert_eq!(
        owner
            .lock()
            .expect("retained original custody")
            .ledger
            .controls_in_flight(),
        1
    );
    assert!(
        lease.try_extend_storage(20, 100),
        "existing admitted custody may grow without minting a role"
    );
    drop(lease);
    let mut state = owner.lock().expect("native release");
    state.ledger.reap_native_controls();
    assert_eq!(state.ledger.controls_in_flight(), 0);
}

#[cfg(windows)]
mod waiting;

#[cfg(windows)]
mod physical;

#[cfg(all(windows, debug_assertions))]
mod x11_capacity;
