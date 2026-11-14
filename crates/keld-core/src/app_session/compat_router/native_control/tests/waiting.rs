//! Actual same-ledger tokens wake the existing synchronous capacity wait.
//! These controls do not claim native `WebView` posting or consumer completion.
use super::*;
use crate::app_session::primary_writer::{PrimaryWriter, WriterRefusal};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn full() -> (
    PrimaryCallerContext,
    Arc<Mutex<CompatSession>>,
    PrimaryWriter,
    Vec<keld_wv::renderer_bridge::NativeControlReservation>,
    mpsc::Receiver<keld_wv::AppWindowCommand>,
) {
    let caller = caller();
    let (session, _outcomes) = super::super::super::test_fixture::availability_session();
    let owner = Arc::new(Mutex::new(session));
    let (progress, received) = mpsc::channel();
    let writer = PrimaryWriter::new(progress);
    owner
        .lock()
        .expect("same ledger")
        .bind_primary_writer(writer.clone())
        .expect("one owner");
    let leases = (0..4)
        .map(|_| {
            owner
                .lock()
                .expect("ledger")
                .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
                .expect("actual original slot")
        })
        .collect();
    (caller, owner, writer, leases, received)
}

#[test]
fn native_release_wakes_waiter_even_with_undispatched_ui_marker() {
    let (caller, owner, writer, mut leases, received) = full();
    writer
        .completed_source_progress()
        .expect("one prior UI marker");
    writer
        .register_native_control_interest()
        .expect("interest before final try");
    assert!(
        owner
            .lock()
            .expect("final ledger")
            .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
            .is_none()
    );
    let (at_wait, waited) = mpsc::sync_channel(1);
    writer.observe_next_capacity_wait_for_test(at_wait);
    let waiting_writer = writer.clone();
    let waiting_caller = caller.clone();
    let (done, finished) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        done.send(waiting_writer.wait_native_control_progress(
            &waiting_caller,
            Instant::now() + Duration::from_secs(3),
        ))
        .expect("wait result");
    });
    waited
        .recv_timeout(Duration::from_secs(2))
        .expect("actual Condvar wait");
    let held = owner.try_lock().expect("no ledger guard crosses wait");
    assert_eq!(held.ledger.controls_in_flight(), 4);
    drop(held);
    drop(leases.pop());
    let result = finished.recv_timeout(Duration::from_secs(2));
    if result.is_err() {
        writer.close();
    }
    thread.join().expect("waiting source ended");
    assert_eq!(result.expect("release wakes without UI pump"), Ok(()));
    let replacement = owner
        .lock()
        .expect("same ledger retry")
        .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
        .expect("only actual token release creates storage");
    assert!(matches!(
        received.try_recv(),
        Ok(keld_wv::AppWindowCommand::HostWriterProgress)
    ));
    assert!(
        received.try_recv().is_err(),
        "no second marker while prior unit is outstanding"
    );
    drop(replacement);
    drop(leases);
}

#[test]
fn release_between_registration_and_wait_and_async_recheck_are_not_lost() {
    for async_register in [false, true] {
        let (caller, owner, writer, mut leases, _received) = full();
        writer
            .completed_source_progress()
            .expect("UI marker stays pending");
        writer
            .register_native_control_interest()
            .expect("synchronous interest");
        assert!(
            owner
                .lock()
                .expect("actual Full")
                .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
                .is_none()
        );
        drop(leases.pop());
        if async_register {
            owner
                .lock()
                .expect("existing async record")
                .register_native_control_interest()
                .expect("register/reap/recheck cannot hide released storage");
        }
        assert_eq!(
            writer
                .wait_native_control_progress(&caller, Instant::now() + Duration::from_millis(100)),
            Ok(())
        );
        assert!(
            owner
                .lock()
                .expect("same retry")
                .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
                .is_some()
        );
        drop(leases);
    }
}

#[test]
fn native_wait_retains_original_deadline_and_role_release_never_refunds_slots() {
    let (caller, owner, writer, leases, _received) = full();
    writer
        .register_native_control_interest()
        .expect("same interest");
    let deadline = Instant::now() + Duration::from_millis(30);
    assert_eq!(
        writer.wait_native_control_progress(&caller, deadline),
        Err(WriterRefusal::Deadline)
    );
    assert_eq!(
        owner
            .lock()
            .expect("retained storage")
            .ledger
            .controls_in_flight(),
        4
    );
    caller.reader_stop.store(true, Ordering::Release);
    writer.role_retired();
    assert_eq!(
        writer.wait_native_control_progress(&caller, Instant::now() + Duration::from_secs(1)),
        Err(WriterRefusal::Retired)
    );
    assert_eq!(
        owner
            .lock()
            .expect("retired storage")
            .ledger
            .controls_in_flight(),
        4
    );
    drop(leases);
}
