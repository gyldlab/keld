//! Completion between pure service and sleep must be re-observed under State.
use super::*;
use std::thread;

#[test]
fn completion_between_source_service_and_sleep_rechecks_existing_slot_serial() {
    let (router, mut peer, _owner, _windows) = initial_ready_router_with_window_loop();
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
                assert!(waiting_writer.shared.state.try_lock().is_ok());
                assert!(waiting_router.current.try_lock().is_ok());
                assert!(waiting_router.shutdown.transition.try_lock().is_ok());
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
    // Wait for actual completion, retaining the source's original receipt. This
    // is the notification deliberately placed before the waiter re-locks State.
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

#[test]
fn enqueue_notifies_every_predicate_waiter_on_the_shared_condition() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    let prepared = frame(&writer, &link, &[4, 2]);
    let (ready, registered) = mpsc::channel();
    let until = deadline();
    let mut waiters = Vec::new();
    for _ in 0..2 {
        let shared = Arc::clone(&writer.shared);
        let ready = ready.clone();
        waiters.push(thread::spawn(move || {
            let mut state = shared.state.lock().expect("shared predicate lock");
            ready.send(()).expect("registration observer");
            loop {
                let remaining = until
                    .checked_duration_since(Instant::now())
                    .unwrap_or_default();
                let (next, timeout) = shared
                    .changed
                    .wait_timeout(state, remaining)
                    .expect("shared notification");
                state = next;
                // Deadline expiry is not a notification or progress proof.
                if timeout.timed_out() {
                    return false;
                }
                if matches!(state.slots[0].phase, queue::Phase::Queued) {
                    return true;
                }
            }
        }));
        registered
            .recv_timeout(Duration::from_secs(2))
            .expect("waiter holds the shared state before sleep");
        // Acquiring the same mutex proves the preceding wait released it.
        drop(writer.shared.state.lock().expect("wait registration fence"));
    }
    let receipt = send(prepared);
    // Keep the receipt and reservation charged: dropping either would introduce
    // another notification and hide the enqueue-only lost-wakeup regression.
    let notified: Vec<_> = waiters
        .into_iter()
        .map(|waiter| waiter.join().expect("bounded waiter"))
        .collect();
    assert_eq!(notified, [true, true]);
    assert_eq!(count(&writer), 1);
    assert!(matches!(
        writer.try_reserve_value(deadline()),
        Err(WriterRefusal::Full)
    ));
    drop(receipt);
}

#[test]
fn actual_writer_advances_when_an_earlier_capacity_waiter_cannot() {
    let (router, mut peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    // Reserving the only VALUE slot creates a genuine blocked producer. Keep
    // the reservation, payload and future receipt charged until after I/O.
    let prepared = frame(&writer, &link, &[0x42, 0x24]);
    let (capacity_observed, capacity_registered) = mpsc::sync_channel(1);
    writer.observe_next_capacity_wait_for_test(capacity_observed);
    let competing = writer.clone();
    let capacity_waiter = thread::spawn(move || {
        competing.reserve_waiting_serviced(
            Class::Value,
            true,
            Instant::now() + Duration::from_secs(3),
            || false,
            || Ok(()),
        )
    });
    capacity_registered
        .recv_timeout(Duration::from_secs(2))
        .expect("real producer reaches full-capacity wait first");
    // The shared mutex fence proves the capacity waiter released it in wait.
    drop(
        writer
            .shared
            .state
            .lock()
            .expect("capacity wait registration"),
    );

    let (idle_observed, idle_registered) = mpsc::sync_channel(1);
    writer
        .shared
        .state
        .lock()
        .expect("idle hook")
        .writer_idle_wait_observer = Some(idle_observed);
    let (writer, mut worker) =
        PrimaryWriterWorker::start_existing_for_test(router, writer).expect("real physical owner");
    idle_registered
        .recv_timeout(Duration::from_secs(2))
        .expect("physical writer reaches second condition wait");
    drop(
        writer
            .shared
            .state
            .lock()
            .expect("writer wait registration"),
    );

    // Without a broadcast, the first waiter can consume the only notification
    // even though its capacity predicate remains false, stranding this worker.
    let receipt = send(prepared);
    peer.set_app_link_read_deadline(Some(Duration::from_secs(2)))
        .expect("bounded physical proof");
    let (header, payload) =
        keld_ipc::link::read_frame(&mut peer).expect("worker physically wrote without release");
    assert_eq!(header.corr, CorrelationId(9));
    assert_eq!(payload, [0x42, 0x24]);
    assert_eq!(
        receipt
            .wait(deadline())
            .expect("same retained receipt settles"),
        WriteCompletion::Written
    );
    assert_eq!(count(&writer), 1, "completed receipt still owns the slot");
    assert!(matches!(
        writer.try_reserve_value(deadline()),
        Err(WriterRefusal::Full)
    ));
    drop(receipt);
    drop(
        capacity_waiter
            .join()
            .expect("bounded genuine capacity waiter")
            .expect("capacity becomes available only after receipt release"),
    );
    worker
        .stop_and_join()
        .expect("real owner terminates cleanly");
}
