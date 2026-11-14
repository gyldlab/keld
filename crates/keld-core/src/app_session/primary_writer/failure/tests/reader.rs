use super::*;

fn source(fixture: &Fixture) -> thread::JoinHandle<Result<(), HostAppError>> {
    let mut reader = fixture
        .router
        .current
        .lock()
        .expect("exact reader source")
        .as_ref()
        .expect("A")
        .writer
        .try_clone()
        .expect("original A reader handle");
    let router = fixture.router.clone();
    let caller = fixture.target.caller().clone();
    fixture.peer.shutdown_app_link().expect("actual EOF");
    thread::spawn(move || app::read_primary_frames(&mut reader, &router, &caller))
}

#[test]
fn actual_current_a_eof_uses_captured_reader_fact_and_exact_owner_cut() {
    let fixture = Fixture::new();
    let reader = source(&fixture);
    let app::DirectPrimaryOwnerCommand::FailPrimaryReader(failure, reply) = fixture
        .owner
        .recv_timeout(Duration::from_secs(3))
        .expect("actual EOF command")
    else {
        panic!("EOF was not captured");
    };
    let restarted = Cell::new(0);
    let disposition = fixture
        .router
        .apply_primary_read_failure(&failure, |attempt| {
            assert_eq!(attempt, 1);
            assert!(fixture.router.shutdown.transition.try_lock().is_err());
            assert!(fixture.router.current.try_lock().is_err());
            restarted.set(restarted.get() + 1);
        })
        .expect("actual caller decision");
    assert_eq!(disposition, FailureDisposition::RestartRequested);
    reply.send(Ok(())).expect("actual owner acknowledgement");
    reader
        .join()
        .expect("reader joined")
        .expect("EOF belongs to runtime owner");
    assert_eq!(restarted.get(), 1);
    assert!(
        fixture
            .router
            .shutdown
            .transition_guard()
            .failed_writer
            .is_none(),
        "EOF must not mint physical write failure evidence"
    );
}

#[test]
#[cfg_attr(
    windows,
    expect(
        clippy::too_many_lines,
        reason = "Keep actual stale EOF, old cleanup, successor identity and same-link wire observations in one scenario."
    )
)]
fn actual_late_a_eof_and_cleanup_cannot_mutate_equal_number_b() {
    let mut fixture = Fixture::new();
    let reader = source(&fixture);
    let original = fixture.target.caller().clone();
    let app::DirectPrimaryOwnerCommand::FailPrimaryReader(failure, reply) = fixture
        .owner
        .recv_timeout(Duration::from_secs(3))
        .expect("observed EOF held before owner decision")
    else {
        panic!("wrong fact");
    };
    fixture
        .router
        .retire_generation_after_write_failure(&original)
        .expect("retire actual A");
    let (server, mut peer) = app::tests::primary_test_stream_pair();
    peer.set_app_link_read_deadline(Some(Duration::from_secs(3)))
        .expect("bounded B observer");
    let ready = thread::spawn(move || {
        let (_, payload) =
            keld_ipc::link::read_frame(&mut peer).expect("actual B lifecycle replay");
        assert_eq!(
            keld_ipc::codec::decode::<app::LifecycleEvent>(&payload).expect("Ready"),
            app::LifecycleEvent::Ready
        );
        peer
    });
    fixture
        .router
        .install_generation(1, server)
        .expect("same-number actual B installer");
    let mut peer = ready.join().expect("B Ready observed");
    let successor = fixture
        .router
        .current_primary_writer_target()
        .expect("B target")
        .expect("B live");
    assert!(!Arc::ptr_eq(
        &successor.caller().reader_stop,
        &original.reader_stop
    ));
    let (echo_reply, echo_observer) = std::sync::mpsc::sync_channel(1);
    *fixture.router.pending_echo.lock().expect("B source record") = Some(app::PendingPrimaryEcho {
        attempt: 1,
        correlation: CorrelationId(93),
        reply: echo_reply,
        numeric: None,
    });
    fixture.router.latch_ready_writer_failure_for(&original);
    assert_eq!(
        fixture
            .router
            .apply_primary_read_failure(&failure, |_| panic!("old EOF restarted B"))
            .expect("late actual EOF decision"),
        FailureDisposition::Ignored
    );
    reply.send(Ok(())).expect("old reader acknowledged inert");
    reader
        .join()
        .expect("A reader joined")
        .expect("old EOF inert");
    fixture
        .router
        .finish_primary_reader_result(
            &original,
            Err(app::app_detail("old A reader", "late cleanup failure")),
        )
        .expect("late error is inert");
    assert!(
        matches!(
            echo_observer.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ),
        "A must not settle B's same-number Echo"
    );
    assert!(!fixture.router.pre_ready_link_failed.load(Ordering::Acquire));
    assert!(
        !fixture
            .windows
            .try_iter()
            .any(|command| matches!(command, AppWindowCommand::Fatal)),
        "A cannot send a global Fatal for B"
    );
    let frame = fixture
        .router
        .primary_writer_owner()
        .expect("same writer")
        .try_prepare_control(
            Arc::clone(&successor),
            ControlMessage::Ping {
                channel: keld_ipc::LIFECYCLE_CHANNEL,
                correlation: CorrelationId(94),
            },
            deadline(),
        )
        .expect("B Ping");
    let receipt = fixture
        .router
        .admit_primary_frame(successor.caller(), frame)
        .expect("B fence")
        .expect("B admitted");
    assert_eq!(
        keld_ipc::link::read_frame(&mut peer)
            .expect("actual B Ping")
            .0
            .corr,
        CorrelationId(94)
    );
    assert_eq!(
        receipt.wait(deadline()).expect("B completion"),
        WriteCompletion::Written
    );
    fixture.target = successor;
}
