//! The initial command owns only its original metadata/reply until real attach.
#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use crate::app_session as app;
use keld_ipc::AppLinkDeadlines;
use std::cell::Cell;
use std::io::Read as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;

fn actual_source() -> (PrimaryRouterHandle, PrimaryReadFailure) {
    let (router, peer, _owner, _window) = app::tests::initial_ready_router_with_window_loop();
    router.window_ready.store(true, Ordering::Release);
    let (mut reader, caller) = {
        let current = router.current.lock().expect("actual initial current");
        let active = current.as_ref().expect("initial A");
        (
            active.writer.try_clone().expect("initial pipe"),
            app::PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            },
        )
    };
    peer.shutdown_app_link().expect("actual initial EOF");
    let error = reader
        .read_exact(&mut [0_u8])
        .expect_err("actual native EOF");
    let failure = PrimaryReadFailure::observed(&caller, error.kind())
        .expect("actual allowed connection-loss kind");
    (router, failure)
}

#[test]
fn original_initial_eof_waits_for_actual_router_attach_then_decides_once() {
    let (router, failure) = actual_source();
    let (reply, receiver) = mpsc::sync_channel(1);
    let mut pending = PendingInitialReadFailure::default();
    pending.retain(failure, reply);
    assert!(
        matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "no decision or acknowledgement before attach"
    );
    let restarts = Cell::new(0);
    pending.attached(&router, |attempt| {
        assert_eq!(attempt, 1);
        assert!(router.shutdown.transition.try_lock().is_err());
        assert!(router.current.try_lock().is_err());
        restarts.set(restarts.get() + 1);
    });
    assert_eq!(receiver.recv().expect("exact retained reply"), Ok(()));
    pending.attached(&router, |_| panic!("initial command replayed"));
    assert_eq!(restarts.get(), 1);
}

#[test]
fn duplicate_initial_report_refuses_without_replacing_original_and_stop_settles_it() {
    let (router, first) = actual_source();
    let (_, duplicate) = actual_source();
    let (reply, receiver) = mpsc::sync_channel(1);
    let (duplicate_reply, duplicate_receiver) = mpsc::sync_channel(1);
    let mut pending = PendingInitialReadFailure::default();
    pending.retain(first, reply);
    pending.retain(duplicate, duplicate_reply);
    assert!(
        duplicate_receiver
            .recv()
            .expect("duplicate refusal")
            .is_err()
    );
    assert!(
        matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "original command remains owned"
    );
    pending.stop("accepted shutdown");
    assert!(receiver.recv().expect("original stop settlement").is_err());
    pending.attached(&router, |_| {
        panic!("stopped initial failure promoted recovery")
    });
}

#[test]
fn original_deadline_expiry_and_owner_drop_settle_without_later_recovery() {
    let (router, failure) = actual_source();
    let original_deadline = failure.deadline;
    let (reply, receiver) = mpsc::sync_channel(1);
    let mut pending = PendingInitialReadFailure::default();
    pending.retain(failure, reply);
    // Only the clock input is controlled; the production expiry predicate and
    // retained original deadline/reply are exactly the ones used by the owner.
    pending.expire_at(original_deadline);
    assert!(
        receiver
            .recv()
            .expect("original deadline settlement")
            .is_err()
    );
    pending.attached(&router, |_| panic!("expired initial EOF promoted recovery"));
    let (_, failure) = actual_source();
    let (reply, receiver) = mpsc::sync_channel(1);
    pending.retain(failure, reply);
    drop(pending);
    assert!(receiver.recv().expect("owner exit settlement").is_err());
}
