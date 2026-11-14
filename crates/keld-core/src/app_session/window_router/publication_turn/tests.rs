//! Actual `WindowSession` turn/CV controls. No native completion is simulated by
//! the turn; caller/Registry authority is still checked by each final publisher.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::compat_router::owned_tests::fixture;
use std::sync::mpsc;
use std::time::Duration;

fn observe_wait(router: &PrimaryRouterHandle) -> mpsc::Receiver<()> {
    let (send, receive) = mpsc::sync_channel(1);
    router
        .windows
        .as_ref()
        .expect("same Registry")
        .lock()
        .expect("same source")
        .publication
        .wait_observer = Some(send);
    receive
}

#[test]
fn held_turn_wait_expires_on_the_original_deadline_without_releasing_its_owner() {
    fixture(|router, caller, _, _, _, _, _| {
        let mut held = Some(
            router
                .claim_window_publication(Instant::now() + Duration::from_secs(3), None)
                .expect("first claim")
                .expect("actual turn"),
        );
        let waiting = observe_wait(router);
        let (finished, completion) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| {
                let deadline = Instant::now() + Duration::from_millis(100);
                let result = router.claim_window_publication(deadline, Some(caller));
                let refused = result.is_err();
                drop(result);
                let _ = finished.send(refused);
            });
            let observed = waiting.recv_timeout(Duration::from_secs(2));
            let completed = completion.recv_timeout(Duration::from_secs(2));
            let still_owned = router
                .windows
                .as_ref()
                .expect("same Registry")
                .lock()
                .expect("wait released Registry guard")
                .publication
                .busy;
            // Cleanup precedes assertions, including a mutant that waited forever.
            drop(held.take());
            waiter
                .join()
                .expect("waiter ended after exact owner release");
            observed.expect("actual Condvar wait, not a scheduling guess");
            assert_eq!(
                completed.ok(),
                Some(true),
                "original deadline must end the wait"
            );
            assert!(still_owned, "timeout cannot release somebody else's turn");
        });
        let next = router
            .claim_window_publication(Instant::now() + Duration::from_secs(1), Some(caller))
            .expect("next original-source turn")
            .expect("first owner actually dropped");
        drop(next);
    });
}

#[test]
fn retired_reader_waking_behind_an_owned_turn_never_claims_it() {
    fixture(|router, caller, _, _, _, _, _| {
        let mut held = Some(
            router
                .claim_window_publication(Instant::now() + Duration::from_secs(3), None)
                .expect("first claim")
                .expect("actual turn"),
        );
        let waiting = observe_wait(router);
        let (finished, completion) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| {
                let result = router.claim_window_publication(
                    Instant::now() + Duration::from_secs(3),
                    Some(caller),
                );
                let stopped = matches!(result, Ok(None));
                drop(result);
                let _ = finished.send(stopped);
            });
            let observed = waiting.recv_timeout(Duration::from_secs(2));
            let retired = router.retire_generation_after_write_failure(caller);
            let current_absent = router.current.lock().expect("actual owner cut").is_none();
            drop(held.take()); // the SAME original owner is the only release
            let completed = completion.recv_timeout(Duration::from_secs(2));
            waiter.join().expect("retired source returned");
            observed.expect("actual held-turn wait");
            retired.expect("actual canonical non-writing retirement");
            assert!(current_absent && caller.reader_stop.load(Ordering::Acquire));
            assert_eq!(
                completed.ok(),
                Some(true),
                "stopped A must not claim on wake"
            );
        });
        assert!(
            !router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("state")
                .publication
                .busy
        );
        // This is cleanup/source ordering only; it grants no successor role.
        let next = router
            .claim_window_publication(Instant::now() + Duration::from_secs(1), None)
            .expect("unowned source turn")
            .expect("released actual turn");
        drop(next);
    });
}

#[test]
fn an_expired_free_turn_refuses_but_an_already_retired_reader_is_benign() {
    fixture(|router, caller, _, _, _, _, _| {
        assert!(
            router
                .claim_window_publication(Instant::now(), Some(caller))
                .is_err()
        );
        assert!(
            !router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("state")
                .publication
                .busy
        );
        router
            .retire_generation_after_write_failure(caller)
            .expect("canonical retirement");
        assert!(matches!(
            router.claim_window_publication(Instant::now(), Some(caller)),
            Ok(None)
        ));
        assert!(
            !router
                .windows
                .as_ref()
                .expect("Registry")
                .lock()
                .expect("state")
                .publication
                .busy
        );
    });
}

#[test]
fn retirement_turn_timeout_still_revokes_and_closes_the_same_actual_link() {
    use std::io::Read as _;
    fixture(|router, caller, _, peer, _, _, _| {
        let mut held = Some(
            router
                .claim_window_publication(Instant::now() + Duration::from_secs(3), None)
                .expect("first claim")
                .expect("actual owner"),
        );
        let waiting = observe_wait(router);
        let (sent, done) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let retirement = scope.spawn(|| {
                let _ = sent.send(router.retire_generation_with_writer(caller.attempt));
            });
            let entered = waiting.recv_timeout(Duration::from_secs(2));
            let result = done.recv_timeout(Duration::from_secs(2));
            let absent_before_turn_release =
                router.current.lock().expect("canonical owner").is_none();
            let stopped_before_turn_release = caller.reader_stop.load(Ordering::Acquire);
            let mut byte = [0_u8; 1];
            let actual_eof = peer.read(&mut byte).is_ok_and(|length| length == 0);
            drop(held.take());
            retirement.join().expect("original retirement returned");
            // Test cleanup cannot turn the captured observations into success.
            router
                .retire_generation_after_write_failure(caller)
                .expect("bounded exact cleanup");
            entered.expect("actual retirement turn wait");
            assert!(
                matches!(result, Ok(Err(_))),
                "the original ordering refusal is preserved"
            );
            assert!(absent_before_turn_release && stopped_before_turn_release);
            assert!(
                actual_eof,
                "timeout still owes actual original-link cleanup, not a timeout receipt"
            );
        });
    });
}
