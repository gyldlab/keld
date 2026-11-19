//! Component controls for pending native preparation and unchanged startup admission.
//! Fake resources prove Core ownership/order only; native retirement has its own cells.

use super::super::{
    APP_LINK_IO_DEADLINE, DirectSessionEnd, HostAppError, SessionShutdownState, app_detail,
    finish_pending_direct_engine, run_direct_startup_if_session_running,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

struct Prepared(Arc<AtomicUsize>);

impl Drop for Prepared {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

#[test]
fn pending_native_preparation_retires_before_its_early_session_result_returns() {
    for failed in [false, true] {
        let drops = Arc::new(AtomicUsize::new(0));
        let calls = AtomicUsize::new(0);
        let result = if failed {
            Err(app_detail("primary startup", "original startup failure"))
        } else {
            Ok(DirectSessionEnd::Ordinary)
        };
        let finished =
            finish_pending_direct_engine(Some(Prepared(Arc::clone(&drops))), result, |prepared| {
                calls.fetch_add(1, Ordering::AcqRel);
                assert_eq!(drops.load(Ordering::Acquire), 0);
                assert!(Arc::ptr_eq(&prepared.0, &drops));
                Ok(())
            });
        assert_eq!(calls.load(Ordering::Acquire), 1, "one native retirement");
        assert_eq!(
            drops.load(Ordering::Acquire),
            1,
            "custody ends before result"
        );
        assert!(finished.retained.is_none());
        assert_eq!(finished.result.is_err(), failed);
        if let Err(error) = finished.result {
            assert!(error.to_string().contains("original startup failure"));
        }
    }
}

#[test]
fn pending_native_retirement_failure_never_replaces_the_original_startup_failure() {
    let drops = Arc::new(AtomicUsize::new(0));
    let finished = finish_pending_direct_engine(
        Some(Prepared(Arc::clone(&drops))),
        Err(app_detail("primary startup", "original startup failure")),
        |prepared| {
            assert!(Arc::ptr_eq(&prepared.0, &drops), "same prepared owner");
            assert_eq!(drops.load(Ordering::Acquire), 0);
            Err(app_detail("native startup retirement", "witness refused"))
        },
    );
    assert_eq!(
        drops.load(Ordering::Acquire),
        0,
        "failed witness dropped exact prepared custody"
    );
    let retained = finished
        .retained
        .expect("failed witness retains the prepared owner");
    assert!(
        Arc::ptr_eq(&retained.0, &drops),
        "original owner reaches the failure"
    );
    let error = finished.result.expect_err("both failures remain visible");
    assert_eq!(error.phase, "primary startup");
    assert!(
        error.to_string().contains("original startup failure"),
        "{error}"
    );
    assert!(error.to_string().contains("witness refused"), "{error}");
    drop(retained);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[test]
fn transferred_engine_is_not_retired_again_and_handover_result_is_preserved() {
    let finished = finish_pending_direct_engine(
        None::<Prepared>,
        Ok(DirectSessionEnd::Handover),
        |_| -> Result<(), HostAppError> { panic!("the native run already owns this engine") },
    );
    assert!(finished.retained.is_none());
    assert_eq!(
        finished.result.expect("completed native run"),
        DirectSessionEnd::Handover
    );
}

#[test]
fn a_cancelled_session_reports_failure_when_prepared_native_retirement_is_unproved() {
    let drops = Arc::new(AtomicUsize::new(0));
    let finished = finish_pending_direct_engine(
        Some(Prepared(Arc::clone(&drops))),
        Ok(DirectSessionEnd::Ordinary),
        |prepared| {
            assert!(Arc::ptr_eq(&prepared.0, &drops));
            Err(app_detail("native startup retirement", "witness refused"))
        },
    );
    assert_eq!(drops.load(Ordering::Acquire), 0);
    let retained = finished
        .retained
        .expect("cancelled session retains unproved custody");
    assert!(Arc::ptr_eq(&retained.0, &drops));
    assert!(
        finished
            .result
            .expect_err("no silent cleanup error")
            .to_string()
            .contains("witness refused")
    );
    drop(retained);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[test]
fn delayed_native_startup_keeps_the_transition_and_a_later_stop_blocks_child_start() {
    let shutdown = SessionShutdownState::new();
    let held_shutdown = shutdown.clone();
    let (entered, observed) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let drops = Arc::new(AtomicUsize::new(0));
    let held_drops = Arc::clone(&drops);
    let startup = thread::spawn(move || {
        run_direct_startup_if_session_running(&held_shutdown, || {
            entered.send(()).expect("report held constructor");
            released
                .recv_timeout(APP_LINK_IO_DEADLINE)
                .expect("release held constructor");
            Prepared(held_drops)
        })
    });
    observed
        .recv_timeout(APP_LINK_IO_DEADLINE)
        .expect("constructor entered");
    let transition_held = matches!(
        shutdown.transition.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    );
    let stop_shutdown = shutdown.clone();
    let (stop_entered, stop_observed) = mpsc::channel();
    let stop = thread::spawn(move || {
        stop_entered.send(()).expect("report pending stop claimant");
        stop_shutdown.claim_cli_lease_lost()
    });
    stop_observed
        .recv_timeout(APP_LINK_IO_DEADLINE)
        .expect("stop claimant entered");
    let still_running_while_held = shutdown.is_running();
    release
        .send(())
        .expect("release deterministic startup hold");
    let prepared = startup.join().expect("startup owner");
    let claimed = stop.join().expect("stop claimant");
    let child_started = AtomicUsize::new(0);
    let child = run_direct_startup_if_session_running(&shutdown, || {
        child_started.fetch_add(1, Ordering::AcqRel);
    });
    let finished =
        finish_pending_direct_engine(prepared, Ok(DirectSessionEnd::Ordinary), |prepared| {
            assert!(Arc::ptr_eq(&prepared.0, &drops));
            Ok(())
        });
    assert!(finished.result.is_ok());
    assert!(finished.retained.is_none());
    assert_eq!(drops.load(Ordering::Acquire), 1);
    assert!(
        transition_held,
        "native preparation lost the existing admission guard"
    );
    assert!(
        still_running_while_held && claimed,
        "stop crossed the held startup admission"
    );
    assert!(child.is_none());
    assert_eq!(
        child_started.load(Ordering::Acquire),
        0,
        "accepted stop forbids a new primary"
    );
}
