use super::*;

#[test]
fn bound_replay_consumes_the_same_fact_inline_and_duplicate_is_ignored() {
    let fixture = Fixture::new();
    let receipt = fixture.actual_unknown();
    let first = receipt
        .failure_evidence()
        .expect("actual failed replay slot")
        .expect("unknown");
    let duplicate = receipt
        .failure_evidence()
        .expect("same exact slot")
        .expect("unknown");
    drop(receipt);
    let failure = |fact| app::PrimaryGenerationFailure {
        error: app::app_detail(
            "retained lifecycle replay",
            "actual original pipe write failed",
        ),
        writer_failure: Some(PrimaryWriterFailure::Write(fact)),
        retired_after_failed_ready: None,
    };
    let restarted = Cell::new(0);
    let consume = |failure| {
        fixture.router.consume_bound_writer_failure(
            1,
            &failure,
            |attempt| {
                assert_eq!(attempt, 1);
                assert!(fixture.router.shutdown.transition.try_lock().is_err());
                assert!(fixture.router.current.try_lock().is_err());
                restarted.set(restarted.get() + 1);
            },
            || panic!("post-Ready original write cannot deny recovery"),
        )
    };
    assert_eq!(
        consume(failure(first)).expect("inline actual owner"),
        Some(FailureDisposition::RestartRequested)
    );
    assert_eq!(
        consume(failure(duplicate)).expect("duplicate inline fact"),
        Some(FailureDisposition::Ignored)
    );
    assert_eq!(restarted.get(), 1);
    assert!(
        matches!(
            fixture.owner.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ),
        "the owner must not recursively submit to its own channel"
    );
}

#[test]
fn known_written_ready_refusal_has_distinct_exact_owner_evidence() {
    let mut fixture = Fixture::new();
    let receipt = fixture.ping(deadline());
    fixture
        .peer
        .set_app_link_read_deadline(Some(Duration::from_secs(3)))
        .expect("bounded actual read");
    let _ = keld_ipc::link::read_frame(&mut fixture.peer).expect("actual physical Written frame");
    assert_eq!(
        receipt.wait(deadline()).expect("actual completion"),
        WriteCompletion::Written
    );
    assert!(
        receipt
            .failure_evidence()
            .expect("successful slot")
            .is_none()
    );
    drop(receipt);
    let failure = {
        let mut transition = fixture.router.shutdown.transition_guard();
        let current = fixture.router.current.lock().expect("exact current owner");
        fixture
            .router
            .record_ready_refusal_locked(
                &mut transition,
                current.as_ref().expect("A"),
                &fixture.target,
            )
            .expect("exact live refusal fact")
    };
    fixture
        .router
        .retire_generation_after_write_failure(fixture.target.caller())
        .expect("same canonical close");
    let failure = PrimaryWriterFailure::Ready(failure);
    let denied = Cell::new(0);
    let result = fixture.router.apply_primary_writer_failure(
        &failure,
        |_| panic!("Ready refusal is not write-recovery authority"),
        || {
            assert!(fixture.router.shutdown.transition.try_lock().is_err());
            denied.set(denied.get() + 1);
        },
    );
    assert_eq!(
        result.expect("exact owner decision"),
        FailureDisposition::RecoveryDenied
    );
    assert_eq!(
        fixture
            .router
            .apply_primary_writer_failure(
                &failure,
                |_| panic!("duplicate restart"),
                || panic!("duplicate denial")
            )
            .expect("duplicate decision"),
        FailureDisposition::Ignored
    );
    assert_eq!(denied.get(), 1);
}

#[test]
fn same_numeric_successor_b_cannot_be_restarted_or_denied_by_late_a() {
    let mut fixture = Fixture::new();
    let receipt = fixture.actual_unknown();
    let failure = receipt
        .failure_evidence()
        .expect("A fact")
        .expect("unknown");
    let late_bound = receipt
        .failure_evidence()
        .expect("same actual A slot")
        .expect("unknown");
    drop(receipt);
    let (server, mut peer) = app::tests::primary_test_stream_pair();
    peer.set_app_link_read_deadline(Some(Duration::from_secs(3)))
        .expect("B bounded read");
    let reader = thread::spawn(move || {
        let (header, payload) =
            keld_ipc::link::read_frame(&mut peer).expect("actual B Ready replay");
        assert_eq!(header.channel, keld_ipc::LIFECYCLE_CHANNEL);
        assert_eq!(
            keld_ipc::codec::decode::<app::LifecycleEvent>(&payload).expect("canonical lifecycle"),
            app::LifecycleEvent::Ready
        );
        peer
    });
    fixture
        .router
        .install_generation(1, server)
        .expect("actual same-number successor installer");
    let mut peer = reader.join().expect("B Ready observer");
    let successor = fixture
        .router
        .current_primary_writer_target()
        .expect("actual B target")
        .expect("live B");
    assert!(!Arc::ptr_eq(
        &successor.caller().reader_stop,
        &fixture.target.caller().reader_stop
    ));
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(failure, &restarted, &denied);
    let late_bound = app::PrimaryGenerationFailure {
        error: app::app_detail("retained lifecycle replay", "late original A failure"),
        writer_failure: Some(PrimaryWriterFailure::Write(late_bound)),
        retired_after_failed_ready: None,
    };
    assert_eq!(
        fixture
            .router
            .consume_bound_writer_failure(
                1,
                &late_bound,
                |_| panic!("late inline A cannot restart B"),
                || panic!("late inline A cannot deny B")
            )
            .expect("late bound owner decision"),
        Some(FailureDisposition::Ignored)
    );

    assert_eq!((restarted.get(), denied.get()), (0, 0));
    let frame = fixture
        .router
        .primary_writer_owner()
        .expect("same physical writer")
        .try_prepare_control(
            Arc::clone(&successor),
            ControlMessage::Ping {
                channel: keld_ipc::LIFECYCLE_CHANNEL,
                correlation: CorrelationId(92),
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
            .expect("actual B frame")
            .0
            .corr,
        CorrelationId(92)
    );
    assert_eq!(
        receipt.wait(deadline()).expect("B completion"),
        WriteCompletion::Written
    );
    fixture.target = successor;
}
