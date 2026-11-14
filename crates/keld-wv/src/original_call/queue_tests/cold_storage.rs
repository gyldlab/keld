//! Actual queue allocation/custody; native execution remains outside this test.
use super::*;

#[test]
fn cold_queue_capacity_stays_charged_through_drain_and_reuse() {
    let mut h = Harness::new();
    let capacity = h.queue.local_calls.capacity();
    assert_eq!(capacity, 64, "existing maximum is allocated once cold");
    let expected = backing::allocation_bytes::<Payload>().unwrap()
        + 64 * size_of::<QueuedOriginalCall<Payload>>();
    assert_eq!(h.queue.retained_bytes(), expected);
    assert_eq!(h.queue.retained_records(), 0);
    assert!(
        h.queue.is_drained(),
        "drained does not mean fixed storage vanished"
    );

    let mut observations = Vec::new();
    for id in 1..=64 {
        observations.push(h.enqueue(id, false));
        assert_eq!(h.queue.local_calls.capacity(), capacity);
    }
    assert_eq!(h.queue.retained_records(), 64);
    assert_eq!(h.queue.local_len(), 64);
    let allocation = h.queue.local_calls.as_slices().0.as_ptr();
    h.queue.cancel_queued().unwrap();
    for body in &observations {
        body.assert_released_once();
    }
    assert_eq!(h.queue.retained_bytes(), expected);
    assert_eq!(h.queue.local_calls.capacity(), capacity);
    assert!(h.queue.is_drained());

    let body = h.enqueue(65, false);
    assert_eq!(h.queue.local_calls.as_slices().0.as_ptr(), allocation);
    assert_eq!(h.queue.local_calls.capacity(), capacity);
    body.assert_held();
    h.queue.cancel_queued().unwrap();
    body.assert_released_once();
    assert!(h.queue.is_drained());
    assert_eq!(h.queue.retained_bytes(), expected);
}

#[test]
fn full_queue_never_grows_and_record_sixty_five_is_refused() {
    let mut h = Harness::new();
    let _bodies: Vec<_> = (1..=64).map(|id| h.enqueue(id, false)).collect();
    let before = (h.queue.retained_bytes(), h.queue.local_calls.capacity());
    let dropped = Arc::new(AtomicUsize::new(0));
    let payload = Payload {
        bytes: String::from("refused65"),
        drops: Arc::clone(&dropped),
    };
    let result = h.queue.enqueue(
        OriginalCallInput {
            id: 65,
            lease: h.lease.clone(),
            payload_bytes: payload.bytes.capacity(),
            payload,
            credential: String::from("app"),
            channel: None,
            role_witness: None,
            facts: None,
        },
        &h.binding,
        &h.owner,
        &h.endpoint,
        None,
        &mut h.fault,
    );
    assert!(result.is_err());
    assert_eq!(h.queue.retained_records(), 64);
    assert_eq!(h.queue.local_len(), 64);
    assert_eq!(
        (h.queue.retained_bytes(), h.queue.local_calls.capacity()),
        before
    );
    assert_eq!(dropped.load(Ordering::Acquire), 1);
    h.queue.cancel_queued().unwrap();
    assert!(h.queue.is_drained());
}

#[test]
fn oversized_cold_layout_refuses_before_constructing_the_queue() {
    //64 occupied8192-byte payload slots already equal the byte maximum,
    // before any required record/backing metadata. No cap is widened.
    assert!(OriginalCallQueue::<[u8; 8192]>::new().is_err());
    let queue = OriginalCallQueue::<Vec<u16>>::new().unwrap();
    assert!(queue.is_drained());
    assert!(queue.retained_bytes() < 524_288);
    assert_eq!(queue.local_calls.capacity(), 64);
}

#[test]
fn fixed_storage_growth_cannot_run_on_an_active_record_or_overflow_the_cap() {
    let mut budget = LocalCallBudget::with_fixed_storage(48).unwrap();
    budget.reserve_fixed_storage(128).unwrap();
    assert_eq!(
        (budget.retained_records(), budget.retained_bytes()),
        (0, 176)
    );
    budget.reserve(1).unwrap();
    assert!(budget.reserve_fixed_storage(1).is_err());
    assert_eq!(
        (budget.retained_records(), budget.retained_bytes()),
        (1, 177)
    );
    budget.release(1).unwrap();
    assert!(budget.reserve_fixed_storage(usize::MAX).is_err());
    assert_eq!(budget.retained_bytes(), 176);
}
