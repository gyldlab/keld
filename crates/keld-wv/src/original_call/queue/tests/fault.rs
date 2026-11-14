use super::*;

#[test]
fn abandoned_preparation_retains_payload_charge_and_blocks_a_second_handle() {
    let mut h = Harness::new();
    let body = h.enqueue(60, false);
    let token = h.take().unwrap();
    let charge = h.queue.retained_bytes();
    drop(token); // Neither explicit no-entry nor actual native return was supplied.
    assert_eq!(h.queue.backing.phase.get(), PreparationPhase::Unproved);
    assert!(
        h.queue
            .take_next_preparation(
                &h.binding,
                &h.owner,
                &h.endpoint,
                (None, None),
                &h.fault,
                |_, _, _, _, _| panic!("unproved backing must not invoke a renderer"),
            )
            .is_err()
    );
    for _ in 0..3 {
        h.fault.latch();
        assert!(h.take().is_none());
        h.queue.cancel_queued().unwrap();
        h.flush();
        assert_eq!(h.queue.retained_bytes(), charge);
        assert!(!h.queue.is_drained());
        body.assert_held();
    }
    // Pure owner-lifetime control. Production retains this whole bridge in its
    // existing fatal session owner; no native Close/release receipt is modeled.
    drop(h);
    body.assert_released_once();
}

#[test]
fn unwound_native_entry_does_not_turn_inline_completion_into_an_api_return() {
    let mut h = Harness::new();
    let body = h.enqueue(61, false);
    let mut token = h.take().unwrap();
    assert!(h.enter(&mut token, OriginalInvocationKind::Original));
    h.finish(61);
    let charge = h.queue.retained_bytes();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _in_call_handle = token;
            panic!("modeled native entry unwind; no physical native proof");
        }))
        .is_err()
    );
    h.flush();
    assert_eq!(h.queue.backing.phase.get(), PreparationPhase::Unproved);
    assert!(h.queue.clear_entered().is_err());
    assert_eq!(h.queue.retained_bytes(), charge);
    assert!(!h.queue.is_drained());
    body.assert_held();
}

#[test]
fn wrong_return_or_defer_phase_never_discards_the_same_owned_body() {
    let mut a = Harness::new();
    let body_a = a.enqueue(62, false);
    let token_a = a.take().unwrap();
    assert!(
        token_a.native_returned().is_err(),
        "Borrowed is not native entry"
    );
    a.flush();
    body_a.assert_held();
    assert_eq!(a.queue.backing.phase.get(), PreparationPhase::Unproved);

    let mut b = a.sharing_source();
    let body_b = b.enqueue(63, false);
    let mut token_b = b.take().unwrap();
    assert!(b.enter(&mut token_b, OriginalInvocationKind::Original));
    assert!(
        token_b.defer().is_err(),
        "Entered cannot be undone as no-entry"
    );
    b.finish(63);
    b.flush();
    body_b.assert_held();
    assert_eq!(b.queue.backing.phase.get(), PreparationPhase::Unproved);
}

#[test]
fn renderer_error_retains_its_actual_partially_prepared_payload_on_the_same_owner() {
    let mut h = Harness::new();
    let body = h.enqueue(64, false);
    let charge = h.queue.retained_bytes();
    assert!(
        h.queue
            .take_next_preparation(
                &h.binding,
                &h.owner,
                &h.endpoint,
                (None, None),
                &h.fault,
                |payload, _, _, _, limit| {
                    assert!(payload.bytes.capacity() <= limit);
                    payload.bytes.clear(); // Same admitted allocation, no replacement.
                    Err(bridge_error("modeled bounded renderer failure"))
                },
            )
            .is_err()
    );
    h.fault.latch();
    h.queue.cancel_queued().unwrap();
    h.flush();
    assert_eq!(h.queue.backing.phase.get(), PreparationPhase::Unproved);
    assert_eq!(h.queue.retained_bytes(), charge);
    assert_eq!(
        h.queue.backing.payload.as_ref().unwrap().bytes.as_ptr() as usize,
        body.allocation
    );
    assert!(h.take().is_none());
    body.assert_held();
}

#[test]
fn faulted_admission_cannot_mint_a_second_handle_without_an_active_record() {
    let mut h = Harness::new();
    let body = h.enqueue(65, false);
    h.fault.latch();
    assert!(!h.queue.has_active());
    assert!(h.take().is_none());
    let rejected_drops = Arc::new(AtomicUsize::new(0));
    let payload = Payload {
        bytes: "unadmitted second payload".into(),
        drops: Arc::clone(&rejected_drops),
    };
    let charge = h.queue.retained_bytes();
    assert!(
        h.queue
            .enqueue(
                OriginalCallInput {
                    id: 66,
                    lease: h.lease.clone(),
                    payload_bytes: payload.bytes.capacity(),
                    payload,
                    credential: "app".into(),
                    channel: None,
                    role_witness: None,
                    facts: None,
                },
                &h.binding,
                &h.owner,
                &h.endpoint,
                None,
                &mut h.fault,
            )
            .is_err()
    );
    assert_eq!(
        rejected_drops.load(Ordering::Acquire),
        1,
        "unadmitted input grants no custody"
    );
    assert_eq!(h.queue.retained_bytes(), charge);
    body.assert_held();
}

#[test]
fn fault_after_preparation_refuses_entry_but_explicit_defer_keeps_available_custody() {
    let mut h = Harness::new();
    let body = h.enqueue(67, false);
    let mut token = h.take().unwrap();
    h.fault.latch();
    assert!(!h.enter(&mut token, OriginalInvocationKind::Original));
    assert!(h.queue.active().is_none());
    h.queue.defer_preparation(token).unwrap();
    assert_eq!(h.queue.backing.phase.get(), PreparationPhase::Available);
    assert!(h.take().is_none());
    body.assert_held();
}
