//! The actual five-position writer supplies the third CONTROL release edge.
use super::*;
use crate::app_session::primary_writer::{ControlMessage, WriterRefusal, admit_locked};
use std::time::{Duration, Instant};

#[test]
fn observer_timeout_and_both_logical_releases_do_not_refund_a_pending_physical_write() {
    super::super::super::owned_tests::fixture(|router, caller, target, peer, writer, _, _| {
        let owner = router.compat.as_ref().expect("same Compat owner");
        let native = {
            let mut state = owner.lock().expect("existing CONTROL");
            state
                .ledger
                .reserve_control(caller, 44, MAX_CONTROL_BYTES)
                .expect("source correlation");
            state
                .ledger
                .lease_existing_control(caller, 44, 0, Some(Arc::clone(owner)))
                .expect("same native half")
        };
        let (dequeued, resume) = writer.hold_next_dequeue_for_test();
        let frame = writer
            .try_prepare_control(
                Arc::clone(target),
                ControlMessage::Ping {
                    channel: keld_ipc::LIFECYCLE_CHANNEL,
                    correlation: CorrelationId(44),
                },
                Instant::now() + Duration::from_secs(3),
            )
            .expect("sole writer frame");
        let receipt = {
            let _transition = router.shutdown.transition_guard();
            let current = router.current.lock().expect("canonical admission");
            let mut state = owner.lock().expect("same control record");
            let receipt =
                admit_locked(current.as_ref().expect("same A"), frame).expect("actual enqueue");
            let record = state
                .ledger
                .controls
                .iter_mut()
                .flatten()
                .find(|record| record.correlation == 44 && record.owned_by(caller))
                .expect("exact source control");
            record.primary_write = Some(receipt.observation());
            receipt
        };
        let observed = receipt.observation();
        let waited = dequeued.recv_timeout(Duration::from_secs(2));
        let timed_out = receipt.wait(Instant::now() + Duration::from_millis(30));
        drop(receipt); // writer owns a real outstanding packet despite observer expiry
        drop(native);
        let retained = {
            let mut state = owner.lock().expect("both logical halves");
            state.ledger.release_control(caller, 44);
            state.ledger.role_retired(caller);
            state.ledger.reap_native_controls();
            state.ledger.controls_in_flight()
        };
        resume
            .send(())
            .expect("release physical writer before red assertions");
        waited.expect("actual dequeue");
        assert_eq!(timed_out, Err(WriterRefusal::Deadline));
        assert_eq!(retained, 1, "observer timeout refunded pending bytes");
        let (header, _) = keld_ipc::link::read_frame(peer).expect("real physical Ping");
        assert_eq!(header.corr, CorrelationId(44));
        observed.wait_quiesced().expect("actual return");
        let mut state = owner.lock().expect("actual reaper");
        state.ledger.reap_native_controls();
        assert_eq!(state.ledger.controls_in_flight(), 0);
    });
}

#[test]
fn written_and_retired_control_stays_owned_until_the_same_late_native_release() {
    super::super::super::owned_tests::fixture(|router, caller, target, peer, writer, _, _| {
        let owner = router.compat.as_ref().expect("same Compat owner");
        let native = {
            let mut state = owner.lock().expect("source control");
            state
                .ledger
                .reserve_control(caller, 45, MAX_CONTROL_BYTES)
                .expect("original correlation");
            state
                .ledger
                .lease_existing_control(caller, 45, 0, Some(Arc::clone(owner)))
                .expect("same native token")
        };
        let frame = writer
            .try_prepare_control(
                Arc::clone(target),
                ControlMessage::Ping {
                    channel: keld_ipc::LIFECYCLE_CHANNEL,
                    correlation: CorrelationId(45),
                },
                Instant::now() + Duration::from_secs(3),
            )
            .expect("one writer frame");
        let receipt = {
            let _transition = router.shutdown.transition_guard();
            let current = router.current.lock().expect("actual A fence");
            let mut state = owner.lock().expect("actual control");
            let receipt =
                admit_locked(current.as_ref().expect("A"), frame).expect("physical admission");
            state
                .ledger
                .controls
                .iter_mut()
                .flatten()
                .find(|record| record.correlation == 45)
                .expect("same record")
                .primary_write = Some(receipt.observation());
            receipt
        };
        let (header, _) = keld_ipc::link::read_frame(peer).expect("actual Ping");
        assert_eq!(header.corr, CorrelationId(45));
        receipt
            .wait(Instant::now() + Duration::from_secs(3))
            .expect("actual physical completion");
        drop(receipt);
        {
            let mut state = owner.lock().expect("retirement cut");
            caller.reader_stop.store(true, Ordering::Release);
            state.ledger.role_retired(caller);
            state.ledger.release_control(caller, 45);
            state.ledger.reap_native_controls();
            assert_eq!(state.ledger.controls_in_flight(), 1);
        }
        drop(native);
        let mut state = owner.lock().expect("same late token");
        state.ledger.reap_native_controls();
        assert_eq!(state.ledger.controls_in_flight(), 0);
    });
}

#[test]
#[cfg_attr(
    windows,
    expect(
        clippy::too_many_lines,
        reason = "Keep the actual CONTROL write, withheld native half, capacity wake and final release in one independently ordered test."
    )
)]
fn physical_control_completion_wakes_same_native_capacity_wait_without_ui_dispatch() {
    super::super::super::owned_tests::fixture(
        |router, caller, target, peer, writer, _, windows| {
            let owner = router.compat.as_ref().expect("same owner");
            let (token, others) = {
                let mut state = owner.lock().expect("four records");
                state
                    .ledger
                    .reserve_control(caller, 46, MAX_CONTROL_BYTES)
                    .expect("correlated source");
                let token = state
                    .ledger
                    .lease_existing_control(caller, 46, 0, Some(Arc::clone(owner)))
                    .expect("same native half");
                let others: Vec<_> = (0..3)
                    .map(|_| {
                        state
                            .reserve_native_control(caller, 1, 1025, Arc::clone(owner))
                            .expect("other original controls")
                    })
                    .collect();
                (token, others)
            };
            let (dequeued, resume) = writer.hold_next_dequeue_for_test();
            let frame = writer
                .try_prepare_control(
                    Arc::clone(target),
                    ControlMessage::Ping {
                        channel: keld_ipc::LIFECYCLE_CHANNEL,
                        correlation: CorrelationId(46),
                    },
                    Instant::now() + Duration::from_secs(3),
                )
                .expect("actual physical frame");
            let receipt = {
                let _transition = router.shutdown.transition_guard();
                let current = router.current.lock().expect("same A");
                let mut state = owner.lock().expect("same CONTROL");
                let receipt =
                    admit_locked(current.as_ref().expect("active"), frame).expect("enqueue");
                state
                    .ledger
                    .controls
                    .iter_mut()
                    .flatten()
                    .find(|record| record.correlation == 46)
                    .expect("same correlation")
                    .primary_write = Some(receipt.observation());
                receipt
            };
            let dequeued = dequeued.recv_timeout(Duration::from_secs(2));
            drop(receipt);
            drop(token);
            owner
                .lock()
                .expect("source completion")
                .ledger
                .release_control(caller, 46);
            writer
                .completed_source_progress()
                .expect("prior UI marker remains undispatched");
            writer
                .register_native_control_interest()
                .expect("register before actual recheck");
            let full = owner
                .lock()
                .expect("final recheck")
                .reserve_native_control(caller, 1, 1025, Arc::clone(owner))
                .is_none();
            let (waited, at_wait) = std::sync::mpsc::sync_channel(1);
            writer.observe_next_capacity_wait_for_test(waited);
            let waiting = writer.clone();
            let captured = caller.clone();
            let (sent, received) = std::sync::mpsc::sync_channel(1);
            let thread = std::thread::spawn(move || {
                sent.send(waiting.wait_native_control_progress(
                    &captured,
                    Instant::now() + Duration::from_secs(2),
                ))
                .expect("bounded wait result");
            });
            let wait_seen = at_wait.recv_timeout(Duration::from_secs(1));
            resume
                .send(())
                .expect("release physical writer before assertions");
            let result = received.recv_timeout(Duration::from_secs(2));
            if result.is_err() {
                writer.close();
            }
            thread.join().expect("waiter returned");
            dequeued.expect("real dequeue");
            assert!(full);
            wait_seen.expect("actual Condvar wait");
            assert_eq!(result.expect("actual completion wake"), Ok(()));
            let (header, _) = keld_ipc::link::read_frame(peer).expect("actual Ping bytes");
            assert_eq!(header.corr, CorrelationId(46));
            let replacement = owner
                .lock()
                .expect("same ledger reaper")
                .reserve_native_control(caller, 1, 1025, Arc::clone(owner));
            assert!(
                replacement.is_some(),
                "only actual quiescence can free this existing slot"
            );
            assert!(matches!(
                windows.try_recv(),
                Ok(keld_wv::AppWindowCommand::HostWriterProgress)
            ));
            assert!(
                windows.try_recv().is_err(),
                "the one UI hint was never dispatched"
            );
            drop(replacement);
            drop(others);
        },
    );
}
