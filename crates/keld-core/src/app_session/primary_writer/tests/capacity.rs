//! Completion between pure service and sleep must be re-observed under State.
use super::*;
use std::thread;

#[test]
fn completion_between_source_service_and_sleep_rechecks_existing_slot_serial() {
    let (router, mut peer, _owner, windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let (writer, mut worker) = PrimaryWriterWorker::start(router.clone()).expect("actual writer");
    let (held, resume) = writer.hold_next_dequeue_for_test();
    let receipt = send(frame(&writer, &link, &[7]));
    held.recv_timeout(Duration::from_secs(3))
        .expect("actual first packet held");
    let source = Arc::new(Mutex::new(RetainedWrite::new(receipt)));
    let original_deadline = Instant::now() + Duration::from_millis(500);
    let (serviced, reached) = mpsc::sync_channel(1);
    let (continue_service, continued) = mpsc::sync_channel(1);
    let waiting_writer = writer.clone();
    let waiting_source = Arc::clone(&source);
    let waiting_router = router.clone();
    let waiter = thread::spawn(move || {
        let mut first = true;
        waiting_writer.reserve_waiting_serviced(
            Class::Value,
            true,
            original_deadline,
            || false,
            || {
                // Every callback uses the same drop(state) cut. Only on the
                // first call is the physical writer still held outside State;
                // after Written it can acquire State again for its next dequeue.
                if first {
                    assert!(waiting_writer.shared.state.try_lock().is_ok());
                    assert!(waiting_router.current.try_lock().is_ok());
                    assert!(waiting_router.shutdown.transition.try_lock().is_ok());
                }
                let _ = waiting_source.lock().expect("test source").settle()?;
                if first {
                    first = false;
                    serviced.send(()).expect("actual pure service reached");
                    continued
                        .recv_timeout(Duration::from_secs(3))
                        .expect("recheck gap released");
                }
                Ok(())
            },
        )
    });
    reached
        .recv_timeout(Duration::from_secs(3))
        .expect("source observed incomplete before gap");
    resume
        .send(())
        .expect("actual write completes in recheck gap");
    assert_eq!(
        keld_ipc::link::read_frame(&mut peer)
            .expect("actual first frame")
            .1,
        [7]
    );
    // Retain the original receipt and verify both Written and the subsequent
    // notification: the sole interested writer posts progress after notify_all.
    {
        let source = source.lock().expect("same retained source");
        let RetainedWrite::Pending(receipt) = &*source else {
            panic!("source settled too early");
        };
        assert_eq!(
            receipt.wait(deadline()).expect("actual completion"),
            WriteCompletion::Written
        );
    }
    assert!(matches!(
        windows.recv_timeout(Duration::from_secs(3)),
        Ok(AppWindowCommand::HostWriterProgress)
    ));
    continue_service
        .send(())
        .expect("release gap after completion notify");
    let reservation = waiter.join().expect("bounded publisher joins");
    let completed = matches!(
        *source.lock().expect("same source"),
        RetainedWrite::Completed(None)
    );
    worker.stop_and_join().expect("actual owner drains");
    assert!(
        reservation.is_ok(),
        "snapshot recheck must not sleep past an already-delivered notification"
    );
    assert!(
        completed,
        "only the same source drops the completed receipt"
    );
    drop(reservation);
}
