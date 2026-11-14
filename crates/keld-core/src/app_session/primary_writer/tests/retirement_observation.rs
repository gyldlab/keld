//! Same-position observations cannot turn an unknown or released outcome into Written.
use super::*;

fn complete_actual_record(writer: &PrimaryWriter, outcome: WriteCompletion) {
    let (position, packet) = writer
        .shared
        .state
        .lock()
        .expect("same fixed queue")
        .dequeue()
        .expect("actual dequeue")
        .expect("admitted packet");
    drop(packet);
    writer
        .shared
        .state
        .lock()
        .expect("same completion owner")
        .complete(position, outcome);
    writer.shared.changed.notify_all();
}

#[test]
fn retirement_observation_preserves_unknown_and_written_without_releasing_receipt() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    for outcome in [WriteCompletion::OutcomeUnknown, WriteCompletion::Written] {
        let receipt = send(frame(&writer, &link, &[1, 2, 3]));
        let observed = receipt.observation();
        complete_actual_record(&writer, outcome);
        assert_eq!(
            observed.wait_quiesced_result().expect("actual quiescence"),
            Some(outcome)
        );
        assert_eq!(
            receipt.poll().expect("same receipt retained"),
            Some(outcome)
        );
        assert_eq!(
            count(&writer),
            1,
            "an observation cannot release the unique receipt"
        );
        drop(receipt);
        assert_eq!(count(&writer), 0);
        assert_eq!(
            observed.wait_quiesced_result().expect("released record"),
            None
        );
    }
}

#[test]
fn released_old_observation_cannot_read_a_reused_positions_written_result() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    let old = send(frame(&writer, &link, &[1]));
    let observed = old.observation();
    complete_actual_record(&writer, WriteCompletion::OutcomeUnknown);
    drop(old);
    let successor = send(frame(&writer, &link, &[2]));
    complete_actual_record(&writer, WriteCompletion::Written);
    assert_eq!(
        observed
            .wait_quiesced_result()
            .expect("old serial quiesced"),
        None,
        "no current or successor completion may stand in for the old source"
    );
    assert_eq!(
        successor.poll().expect("actual new receipt"),
        Some(WriteCompletion::Written)
    );
    drop(successor);
    assert_eq!(count(&writer), 0);
}
