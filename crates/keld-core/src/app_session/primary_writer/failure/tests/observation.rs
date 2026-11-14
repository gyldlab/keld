//! Actual receipt observations cannot submit recovery while a source owns order.
use super::*;

#[test]
fn written_observation_keeps_physical_time_and_preserves_accepted_tail() {
    let mut fixture = Fixture::new();
    let original_deadline = deadline();
    let receipt = fixture.ping(original_deadline);
    fixture
        .peer
        .set_app_link_read_deadline(Some(Duration::from_secs(3)))
        .expect("bounded original pipe read");
    let (header, _) = keld_ipc::link::read_frame(&mut fixture.peer).expect("physical Ping");
    assert_eq!(header.corr, CorrelationId(91));
    assert_eq!(
        receipt.wait(original_deadline).expect("actual completion"),
        WriteCompletion::Written
    );
    let physical = receipt
        .written_at()
        .expect("same slot timestamp")
        .expect("Written timestamp");
    {
        let _transition = fixture.router.shutdown.transition_guard();
        assert!(
            fixture
                .router
                .shutdown
                .claim_guarded(app::SESSION_LIFECYCLE_QUIT)
        );
    }
    let (result, failure) = fixture
        .router
        .observe_primary_write(&fixture.target, receipt, original_deadline, "observed Ping")
        .into_parts();
    assert_eq!(result.expect("known Written remains Written"), physical);
    assert!(
        failure.is_none(),
        "accepted tail cannot acquire fabricated failed-write authority"
    );
    assert!(
        fixture
            .router
            .current
            .lock()
            .expect("retained accepted A")
            .is_some()
    );
    assert!(
        fixture.owner.try_recv().is_err(),
        "observation sends no owner command"
    );
}

#[test]
fn unknown_observation_retains_sealed_failure_without_owner_rpc() {
    let fixture = Fixture::new();
    let receipt = fixture.actual_unknown();
    let original_deadline = receipt.deadline;
    let (result, failure) = fixture
        .router
        .observe_primary_write(
            &fixture.target,
            receipt,
            original_deadline,
            "observed failed Ping",
        )
        .into_parts();
    assert!(result.is_err());
    let failure = failure.expect("actual unknown receipt supplies its same sealed fact");
    assert!(
        fixture.owner.try_recv().is_err(),
        "publication turn cannot wait on DirectPrimaryOwner"
    );
    assert!(
        fixture
            .router
            .shutdown
            .transition_guard()
            .failed_writer
            .is_some()
    );
    assert_eq!(
        fixture
            .router
            .primary_writer_owner()
            .expect("writer")
            .shared
            .state
            .lock()
            .expect("actual completed receipt freed")
            .charged(),
        0
    );
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(failure, &restarted, &denied);
    assert_eq!(
        (restarted.get(), denied.get()),
        (1, 0),
        "only later source reporting requests recovery"
    );
}

#[test]
fn expired_observation_revokes_exact_a_but_retains_actual_pending_writer_charge() {
    let fixture = Fixture::new();
    let writer = fixture.router.primary_writer_owner().expect("writer");
    let (held, resume) = writer.hold_next_dequeue_for_test();
    let original_deadline = Instant::now() + Duration::from_millis(60);
    let receipt = fixture.ping(original_deadline);
    held.recv_timeout(Duration::from_secs(3))
        .expect("actual pending writer");
    let observation = receipt.observation();
    let (result, failure) = fixture
        .router
        .observe_primary_write(
            &fixture.target,
            receipt,
            original_deadline,
            "observed admitted expiry",
        )
        .into_parts();
    let charged = writer.shared.state.lock().expect("pending slot").charged();
    let current_absent = fixture
        .router
        .current
        .lock()
        .expect("exact A owner")
        .is_none();
    let command_absent = fixture.owner.try_recv().is_err();
    resume
        .send(())
        .expect("release native worker before assertions");
    observation
        .wait_quiesced()
        .expect("actual pending call returned");
    assert!(result.is_err());
    assert!(current_absent);
    assert!(command_absent, "record/revoke does not submit recovery");
    assert_eq!(
        charged, 1,
        "observer expiry and receipt drop cannot free a live packet"
    );
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(
        failure.expect("original admitted expiry fact"),
        &restarted,
        &denied,
    );
    assert_eq!((restarted.get(), denied.get()), (1, 0));
}
