//! Both native-release/interest orders use one writer wake, never a second queue.
use super::*;
use crate::app_session::primary_writer::PrimaryWriter;
use keld_wv::AppWindowCommand;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn native_control_release_and_register_recheck_cannot_lose_or_duplicate_wake() {
    for release_first in [false, true] {
        let caller = caller();
        let (session, _outcomes) = super::super::super::test_fixture::availability_session();
        let owner = Arc::new(Mutex::new(session));
        let (progress, received) = mpsc::channel();
        let writer = PrimaryWriter::new(progress);
        owner
            .lock()
            .expect("same ledger")
            .bind_primary_writer(writer.clone())
            .expect("one writer wake owner");
        let mut leases = Vec::new();
        for _ in 0..4 {
            leases.push(
                owner
                    .lock()
                    .expect("same slot owner")
                    .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
                    .expect("one original control"),
            );
        }
        let mut state = owner.lock().expect("final Full/interest fence");
        assert!(
            state
                .reserve_native_control(&caller, 1, 1025, Arc::clone(&owner))
                .is_none()
        );
        let first = leases.pop().expect("native side release token");
        if release_first {
            // The native thread holds no ledger guard. Core deliberately still
            // holds it across the Full result and subsequent registration cut.
            let (done, released) = mpsc::sync_channel(1);
            let native = std::thread::spawn(move || {
                drop(first);
                done.send(()).expect("actual native storage released");
            });
            released
                .recv_timeout(Duration::from_secs(2))
                .expect("Drop never waits for Core ledger");
            native.join().expect("native release worker");
            assert!(
                received.try_recv().is_err(),
                "release without retained interest queues no marker"
            );
            state
                .register_native_control_interest()
                .expect("register then recheck catches prior release");
        } else {
            state
                .register_native_control_interest()
                .expect("interest before native release");
            assert!(received.try_recv().is_err());
            drop(state);
            drop(first);
            state = owner.lock().expect("observe actual slot after release");
        }
        assert!(matches!(
            received
                .recv_timeout(Duration::from_secs(2))
                .expect("actual unit progress"),
            AppWindowCommand::HostWriterProgress
        ));
        drop(state);
        drop(leases.pop());
        drop(leases.pop());
        assert!(
            received.try_recv().is_err(),
            "native releases coalesce while the marker is outstanding"
        );
        writer
            .acknowledge_progress()
            .expect("unit acknowledgement only");
        let mut state = owner.lock().expect("service actual retained ledger");
        state.ledger.reap_native_controls();
        assert_eq!(state.ledger.controls_in_flight(), 1);
        assert!(
            received.try_recv().is_err(),
            "acknowledgement is not a fabricated receipt"
        );
        drop(state);
        drop(leases);
    }
}
