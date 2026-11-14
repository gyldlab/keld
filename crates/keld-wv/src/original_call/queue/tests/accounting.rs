use super::*;

#[test]
fn cold_backing_uses_the_same_byte_limit_without_consuming_a_call_record() {
    let mut queue = OriginalCallQueue::<String>::new().unwrap();
    let fixed = backing::allocation_bytes::<String>().unwrap()
        + 64 * std::mem::size_of::<QueuedOriginalCall<String>>();
    assert_eq!(
        (queue.retained_records(), queue.retained_bytes()),
        (0, fixed)
    );
    assert!(queue.is_drained());
    assert!(
        !queue.local_budget.is_empty(),
        "fixed allocation remains accounted while queue lives"
    );
    let bytes = super::super::super::MAX_LOCAL_CALL_BYTES - fixed;
    queue.local_budget.reserve(bytes).unwrap();
    assert!(queue.local_budget.reserve(1).is_err());
    assert_eq!(
        queue.retained_bytes(),
        super::super::super::MAX_LOCAL_CALL_BYTES
    );
    queue.local_budget.release(bytes).unwrap();
    assert_eq!(
        (queue.retained_records(), queue.retained_bytes()),
        (0, fixed)
    );
    assert!(queue.is_drained());
}

#[test]
fn completed_successors_reuse_the_exact_cold_backing_allocation() {
    let mut h = Harness::new();
    let backing = Rc::as_ptr(&h.queue.backing);
    let baseline = h.queue.retained_bytes();
    for id in 70..73 {
        let body = h.enqueue(id, false);
        let mut token = h.take().unwrap();
        assert_eq!(Rc::as_ptr(&token.backing), backing);
        assert_eq!(Rc::strong_count(&token.backing), 2);
        assert!(h.enter(&mut token, OriginalInvocationKind::Original));
        token.native_returned().unwrap();
        h.finish(id);
        h.flush();
        body.assert_released_once();
        assert_eq!(Rc::as_ptr(&h.queue.backing), backing);
        assert_eq!(Rc::strong_count(&h.queue.backing), 1);
        assert_eq!(
            (h.queue.retained_records(), h.queue.retained_bytes()),
            (0, baseline)
        );
        assert!(h.queue.is_drained());
    }
}

#[test]
fn same_record_charge_includes_coexisting_metadata_and_immutable_handle() {
    let mut h = Harness::new();
    let before = h.queue.retained_bytes();
    h.enqueue(73, false);
    let call = h.queue.local_calls.front().unwrap();
    let metadata = std::mem::size_of::<ActiveOriginalRecord<Payload>>()
        .max(std::mem::size_of::<QueuedOriginalCall<Payload>>())
        .max(std::mem::size_of::<PendingOriginalCall<Payload>>())
        + std::mem::size_of::<OriginalQueueProgress<Option<PreparingOriginalInvocation<Payload>>>>(
        );
    let exact = call.payload_bytes + call.credential.capacity() + metadata;
    assert_eq!(call.bytes, exact);
    assert_eq!(h.queue.retained_bytes(), before + exact);
    assert_eq!(h.queue.retained_records(), 1);
}
