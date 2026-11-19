//! A timeout, poisoned observation, or unwinding worker is not early release.
use super::*;
use std::thread;

fn observe_wait(writer: &PrimaryWriter) -> mpsc::Receiver<()> {
    let (sent, received) = mpsc::sync_channel(1);
    writer
        .shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .quiescence_wait_observer = Some(sent);
    received
}

#[test]
fn actual_quiescence_outlives_original_deadline_until_held_worker_returns() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let (writer, mut worker) = PrimaryWriterWorker::start(router).expect("actual writer");
    let (held, resume) = writer.hold_next_dequeue_for_test();
    let original = Instant::now() + Duration::from_millis(80);
    let prepared = writer
        .try_reserve_value(original)
        .expect("sole value")
        .copy_value(
            link,
            FrameKind::Reply,
            keld_ipc::ECHO_CHANNEL,
            CorrelationId(91),
            &[1],
        )
        .expect("exact charged packet");
    let receipt = send(prepared);
    held.recv_timeout(Duration::from_secs(3))
        .expect("actual worker holds packet");
    assert_eq!(receipt.wait(original), Err(WriterRefusal::Deadline));
    let observation = receipt.observation();
    let waiting = observe_wait(&writer);
    let (done, result) = mpsc::sync_channel(1);
    let observer = thread::spawn(move || {
        let _ = done.send(observation.wait_quiesced());
    });
    // Always release the actual worker before assertions, including a red mutant.
    let entered_wait = waiting.recv_timeout(Duration::from_secs(1));
    let premature = result.try_recv();
    let charged_before_return = count(&writer);
    resume.send(()).expect("release original physical owner");
    let completion = if let Ok(value) = premature {
        value
    } else {
        result
            .recv_timeout(Duration::from_secs(3))
            .expect("actual return notifies observer")
    };
    observer.join().expect("observer joins");
    let actual = receipt.wait(deadline());
    let charged_after_return = count(&writer);
    drop(receipt);
    let charged_after_receipt = count(&writer);
    worker.stop_and_join().expect("actual owner drained");
    assert!(
        entered_wait.is_ok(),
        "expired clock must still observe actual pending slot"
    );
    assert!(
        matches!(premature, Err(mpsc::TryRecvError::Empty)),
        "deadline cannot prove quiescence"
    );
    assert_eq!(completion, Ok(()));
    assert_eq!(actual, Ok(WriteCompletion::OutcomeUnknown));
    assert_eq!(charged_before_return, 1);
    assert_eq!(
        charged_after_return, 1,
        "completed receipt still owns credit"
    );
    assert_eq!(charged_after_receipt, 0);
}

#[test]
fn real_worker_unwind_settles_actual_slot_without_releasing_retained_receipt() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let (writer, mut worker) = PrimaryWriterWorker::start(router).expect("actual writer");
    let (arrived, held) = mpsc::sync_channel(1);
    let (resume, proceed) = mpsc::sync_channel(1);
    writer.shared.state.lock().expect("test hook").after_dequeue =
        Some(Arc::new(worker::DequeueHook {
            arrived,
            resume: Mutex::new(proceed),
            panic_after_resume: true,
        }));
    let receipt = send(frame(&writer, &link, &[2]));
    held.recv_timeout(Duration::from_secs(3))
        .expect("actual worker holds packet");
    let observation = receipt.observation();
    let waiting = observe_wait(&writer);
    let (done, result) = mpsc::sync_channel(1);
    let observer = thread::spawn(move || {
        let _ = done.send(observation.wait_quiesced());
    });
    waiting
        .recv_timeout(Duration::from_secs(3))
        .expect("same slot wait before unwind");
    resume
        .send(())
        .expect("panic actual worker after retained packet");
    // A missing unwind cleanup mutant must fail boundedly, not hang the suite.
    let settled = result.recv_timeout(Duration::from_secs(1));
    let joined = worker.stop_and_join();
    if settled.is_err() {
        let wake = writer
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .worker_ended();
        writer.shared.changed.notify_all();
        writer.shared.wake(wake);
    }
    observer
        .join()
        .expect("observer joins after actual or fixture cleanup");
    let actual = receipt.poll();
    let retained = count(&writer);
    drop(receipt);
    assert!(
        settled.is_ok(),
        "outer worker guard must prove cleanup during unwind"
    );
    assert!(joined.is_err(), "actual worker panic remains an error");
    assert_eq!(actual, Ok(Some(WriteCompletion::OutcomeUnknown)));
    assert_eq!(retained, 1, "unwind never refunds a retained receipt");
    assert_eq!(count(&writer), 0);
}

#[test]
fn poisoned_observation_reports_error_only_after_actual_worker_quiescence() {
    let (router, mut peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let (writer, mut worker) = PrimaryWriterWorker::start(router).expect("actual writer");
    let (held, resume) = writer.hold_next_dequeue_for_test();
    let receipt = send(frame(&writer, &link, &[3]));
    held.recv_timeout(Duration::from_secs(3))
        .expect("actual worker holds packet");
    let waiting = observe_wait(&writer);
    let shared = Arc::clone(&writer.shared);
    assert!(
        thread::spawn(move || {
            let _state = shared.state.lock().expect("poison actual state mutex");
            panic!("controlled observer mutex poison");
        })
        .join()
        .is_err()
    );
    let observation = receipt.observation();
    let (done, result) = mpsc::sync_channel(1);
    let observer = thread::spawn(move || {
        let _ = done.send(observation.wait_quiesced());
    });
    let entered_wait = waiting.recv_timeout(Duration::from_secs(1));
    let premature = result.try_recv();
    resume
        .send(())
        .expect("allow actual native write to return");
    let physical = keld_ipc::link::read_frame(&mut peer);
    let completion = if let Ok(value) = premature {
        value
    } else {
        result
            .recv_timeout(Duration::from_secs(3))
            .expect("actual return notifies poisoned observer")
    };
    observer.join().expect("observer joins");
    let _ = worker.stop_and_join();
    let quiesced = !writer
        .shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pending(receipt.position);
    drop(receipt);
    assert!(
        entered_wait.is_ok(),
        "poison cannot be used as physical completion"
    );
    assert!(matches!(premature, Err(mpsc::TryRecvError::Empty)));
    assert_eq!(physical.expect("actual original bytes").1, [3]);
    assert_eq!(completion, Err(WriterRefusal::Poisoned));
    assert!(quiesced);
}
