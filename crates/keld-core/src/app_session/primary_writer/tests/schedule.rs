use super::super::super::with_primary_caller_locked;
use super::*;
use std::io::Read as _;
use std::thread;

fn hold_next_dequeue(writer: &PrimaryWriter) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (arrived, observe) = mpsc::sync_channel(1);
    let (resume, proceed) = mpsc::sync_channel(1);
    writer
        .shared
        .state
        .lock()
        .expect("dequeue hook")
        .after_dequeue = Some(Arc::new(worker::DequeueHook {
        arrived,
        resume: Mutex::new(proceed),
        panic_after_resume: false,
    }));
    (observe, resume)
}
fn assert_no_bytes(peer: &mut keld_ipc::BootstrapStream) {
    peer.set_app_link_read_deadline(Some(Duration::from_millis(40)))
        .expect("bounded negative observation");
    let mut byte = [0];
    let error = peer
        .read(&mut byte)
        .expect_err("no physical byte may precede commit");
    assert!(
        matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ),
        "{error}"
    );
    peer.set_app_link_read_deadline(Some(Duration::from_secs(3)))
        .expect("restore absolute test budget");
}

#[test]
fn queued_value_reaches_physical_peer_with_a_competing_capacity_waiter() {
    let (router, mut peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    // Reserve VALUE while encoding. A second producer is left waiting on the
    // same Condvar as the idle physical worker before this frame is admitted.
    let prepared = frame(&writer, &link, &[0x42, 0x43]);
    let (observed_tx, observed_rx) = mpsc::sync_channel(1);
    writer.observe_next_capacity_wait_for_test(observed_tx);
    let waiting_writer = writer.clone();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let waiter = thread::spawn(move || {
        let reserved = waiting_writer.reserve_waiting(
            Class::Value,
            true,
            Instant::now() + Duration::from_secs(3),
            || false,
        );
        result_tx
            .send(reserved.is_ok())
            .expect("capacity waiter result receiver");
        reserved
    });
    observed_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("capacity waiter encountered the occupied VALUE slot");
    // This lock turn completes after the observed producer released State to
    // wait; the queued packet must wake the consumer even if this waiter wakes.
    drop(
        writer
            .shared
            .state
            .lock()
            .expect("capacity waiter released State"),
    );
    let (idle_tx, idle_rx) = mpsc::sync_channel(1);
    writer
        .shared
        .state
        .lock()
        .expect("worker observation")
        .worker_wait_observer = Some(idle_tx);
    let (_, mut worker) =
        PrimaryWriterWorker::start_existing_for_test(router.clone(), writer.clone())
            .expect("real worker with the already-blocked producer");
    idle_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("physical worker reached its idle condition-variable wait");
    drop(
        writer
            .shared
            .state
            .lock()
            .expect("physical worker released State to wait"),
    );
    let receipt = send(prepared);
    let (header, body) =
        keld_ipc::link::read_frame(&mut peer).expect("physical worker must wake for queued VALUE");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(9))
    );
    assert_eq!(body, [0x42, 0x43]);
    assert_eq!(
        receipt.wait(deadline()).expect("physical write completion"),
        WriteCompletion::Written
    );
    assert!(
        result_rx.try_recv().is_err(),
        "physical completion cannot refund the still-retained VALUE receipt"
    );
    drop(receipt);
    assert!(
        result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("capacity release"),
        "retiring the written receipt must free the waiter"
    );
    drop(
        waiter
            .join()
            .expect("capacity producer joined")
            .expect("reservation"),
    );
    worker.stop_and_join().expect("physical consumer joined");
}

#[test]
fn actual_worker_dequeues_but_cannot_write_until_terminal_commit_releases_fence() {
    let (router, mut peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let (writer, mut worker) =
        PrimaryWriterWorker::start(router.clone()).expect("one real physical writer");
    let (dequeued, resume) = hold_next_dequeue(&writer);
    let prepared = frame(&writer, &link, &[9, 8, 7]);
    let transition = router.shutdown.transition_guard();
    let current = router.current.lock().expect("terminal publication fence");
    let receipt = send(prepared);
    dequeued
        .recv_timeout(Duration::from_secs(2))
        .expect("worker actually dequeued before commit");
    assert_eq!(count(&writer), 1);
    resume.send(()).expect("release worker to canonical fence");
    assert_no_bytes(&mut peer);
    assert_eq!(
        receipt.poll().expect("no completion before publication"),
        None
    );
    let committed = AtomicUsize::new(1);
    drop(current);
    drop(transition);
    let (header, payload) =
        keld_ipc::link::read_frame(&mut peer).expect("actual original pipe frame");
    assert_eq!(committed.load(Ordering::SeqCst), 1);
    assert_eq!(header.corr, CorrelationId(9));
    assert_eq!(payload, [9, 8, 7]);
    assert_eq!(
        receipt
            .wait(deadline())
            .expect("actual physical completion"),
        WriteCompletion::Written
    );
    assert!(router.shutdown.transition.try_lock().is_ok());
    assert!(router.current.try_lock().is_ok());
    assert_eq!(count(&writer), 1, "retained completion remains charged");
    drop(receipt);
    worker
        .stop_and_join()
        .expect("join only after all Core fences released");
}

#[test]
fn waiter_timeout_and_role_retirement_do_not_refund_the_dequeued_position() {
    let (router, mut peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let (writer, mut worker) = PrimaryWriterWorker::start(router.clone()).expect("writer");
    let (dequeued, resume) = hold_next_dequeue(&writer);
    let receipt = send(frame(&writer, &link, &[6]));
    dequeued
        .recv_timeout(Duration::from_secs(2))
        .expect("dequeued original payload");
    assert_eq!(
        receipt.wait(Instant::now() + Duration::from_millis(1)),
        Err(WriterRefusal::Deadline)
    );
    drop(receipt);
    writer.role_retired();
    assert_eq!(count(&writer), 1);
    assert!(matches!(
        writer.try_reserve_value(deadline()),
        Err(WriterRefusal::Full)
    ));
    resume.send(()).expect("actual write may finish");
    assert_eq!(
        keld_ipc::link::read_frame(&mut peer)
            .expect("original physical frame")
            .1,
        [6]
    );
    let until = deadline();
    let mut state = writer.shared.state.lock().expect("completion state");
    while state.charged() != 0 {
        let remaining = until
            .checked_duration_since(Instant::now())
            .expect("actual abandoned completion reclaimed");
        state = writer
            .shared
            .changed
            .wait_timeout(state, remaining)
            .expect("completion notification")
            .0;
    }
    drop(state);
    drop(
        writer
            .try_reserve_value(deadline())
            .expect("native completion finally freed the abandoned slot"),
    );
    worker.stop_and_join().expect("drained owner");
}

#[test]
fn retire_first_canonical_caller_fence_never_freshly_admits_the_prepared_frame() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    let mut prepared = Some(frame(&writer, &link, &[4]));
    router
        .retire_generation(link.caller().attempt)
        .expect("actual retirement wins first");
    with_primary_caller_locked(&router.current, &router.shutdown, link.caller(), |_| {
        let _ = send(prepared.take().expect("prepared frame"));
        panic!("stopped caller was freshly admitted");
    })
    .expect("canonical stale caller refusal");
    assert!(
        prepared.is_some(),
        "never-written frame remains with its original preparer"
    );
    assert!(
        writer
            .shared
            .state
            .lock()
            .expect("actual FIFO")
            .dequeue()
            .expect("queue")
            .is_none()
    );
    drop(prepared);
    assert_eq!(count(&writer), 0);
}

#[test]
fn admitted_frame_keeps_original_a_target_and_receipt_when_b_becomes_current() {
    let (router, _a_peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let original = target(&router);
    let (writer, mut worker) = PrimaryWriterWorker::start(router.clone()).expect("writer");
    let (dequeued, resume) = hold_next_dequeue(&writer);
    let mut frame = Some(frame(&writer, &original, &[0xa1, 0xa2]));
    let mut receipt = None;
    with_primary_caller_locked(&router.current, &router.shutdown, original.caller(), |_| {
        receipt = Some(send(frame.take().expect("prepared A frame")));
        Ok(())
    })
    .expect("A wins final admission");
    dequeued
        .recv_timeout(Duration::from_secs(2))
        .expect("A physical frame held");
    router
        .retire_generation(1)
        .expect("canonical A retirement closes A only");
    let (b_server, mut b_peer) = super::super::super::tests::primary_test_stream_pair();
    router
        .install_generation(2, b_server)
        .expect("actual existing successor installer");
    assert_eq!(
        count(&writer),
        1,
        "restart cannot reset the session-wide position"
    );
    resume.send(()).expect("late A completion");
    let receipt = receipt.expect("A receipt");
    assert_eq!(
        receipt.wait(deadline()).expect("closed A link settles"),
        WriteCompletion::OutcomeUnknown
    );
    assert!(
        router.is_current(2),
        "old failure cannot retire or redirect to B"
    );
    assert_no_bytes(&mut b_peer);
    assert_eq!(count(&writer), 1, "ambiguous A completion retained");
    drop(receipt);
    worker.stop_and_join().expect("single worker joined");
    router
        .retire_generation(2)
        .expect("close successor fixture");
    let readers = std::mem::take(&mut *router.readers.lock().expect("reader custody"));
    for (_, reader) in readers {
        reader
            .join()
            .expect("fixture reader exited")
            .expect("fixture reader result");
    }
}
