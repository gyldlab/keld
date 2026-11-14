use super::*;

#[test]
fn revoked_and_foreign_same_stamp_leases_do_not_admit_or_redirect_result() {
    let mut h = Harness::new();
    h.begin(10);
    let mut request = h.handoff();
    let token = request.take_numeric_result().unwrap();
    let mut foreign = NativeDocumentOwner::new(WebviewId(7));
    foreign.bind_source(&h.fixture.source_arc()).unwrap();
    let attempt = foreign
        .begin_host_load(h.fixture.source_arc().source_id())
        .unwrap();
    let stamp = foreign
        .commit_admitted(
            attempt,
            h.fixture.source_arc().source_id(),
            NativeCommitIdentity(NonZeroU64::new(17).unwrap()),
        )
        .unwrap();
    let foreign_lease = CheckedNativeDocumentLease(foreign.lease_for(stamp).unwrap());
    assert!(!token.matches_request(WebviewId(7), h.navigation(), 10, &foreign_lease));
    h.owner
        .begin_host_load(h.fixture.source_arc().source_id())
        .unwrap();
    h.queue.reconcile_numeric_owner(&h.owner);
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    assert!(
        token.matches_request(WebviewId(7), h.navigation(), 10, &h.lease),
        "original cleanup remains correlated after revocation"
    );
    drop(request);
    drop(token);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    let mut stale = h.request(11);
    assert!(
        h.queue
            .begin_numeric_result(&mut stale, &h.owner, None, None)
            .is_err()
    );
    assert!(stale.unwrap().numeric_result.is_none());
}

#[test]
fn numeric_record_uses_existing_byte_and_record_limits_without_extra_capacity() {
    let mut h = Harness::new();
    let input_capacity = h.request(12).unwrap().payload.capacity();
    let charge = record_bytes(input_capacity).unwrap();
    let occupied = crate::original_call::MAX_LOCAL_CALL_BYTES - h.fixed() - charge + 1;
    h.queue.local_budget.reserve(occupied).unwrap();
    let before = h.queue.retained_bytes();
    let mut input = h.request(12);
    assert!(
        h.queue
            .begin_numeric_result(&mut input, &h.owner, None, None)
            .is_err()
    );
    assert_eq!(h.queue.retained_bytes(), before);
    assert!(input.as_ref().unwrap().numeric_result.is_none());
    h.queue.local_budget.release(occupied).unwrap();
    for _ in 0..63 {
        h.queue.local_budget.reserve(0).unwrap();
    }
    h.queue
        .begin_numeric_result(&mut input, &h.owner, None, None)
        .unwrap();
    assert_eq!(h.queue.retained_records(), 64);
    assert!(h.queue.local_budget.reserve(0).is_err());
    h.queue.cancel_numeric_result();
    drop(h.queue.take_cancelled_numeric_request().unwrap());
    assert!(h.queue.reconcile_numeric_result().unwrap());
    for _ in 0..63 {
        h.queue.local_budget.release(0).unwrap();
    }
    assert!(h.queue.is_drained());
}

#[test]
fn existing_app_preflight_waits_for_same_numeric_record_physical_release() {
    let mut h = Harness::new();
    h.begin(13);
    let mut fault = OriginalControlFault::default();
    h.queue
        .enqueue(
            OriginalCallInput {
                id: 14,
                lease: h.lease.clone(),
                payload: vec![],
                payload_bytes: 0,
                credential: "app".into(),
                channel: Some("rendererReq".into()),
                role_witness: Some(h.role.clone()),
                facts: None,
            },
            &h.binding,
            &h.owner,
            &h.endpoint,
            None,
            &mut fault,
        )
        .unwrap();
    assert!(!h.queue.has_pending());
    let progress = h
        .queue
        .take_next_preparation(
            &h.binding,
            &h.owner,
            &h.endpoint,
            (None, None),
            &fault,
            |payload, _, _, _, _| Ok(payload.capacity()),
        )
        .unwrap();
    assert!(progress.value.is_none());
    assert!(matches!(
        h.receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    h.queue.cancel_numeric_result();
    let original = h.queue.take_cancelled_numeric_request().unwrap();
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    drop(original);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    let progress = h
        .queue
        .take_next_preparation(
            &h.binding,
            &h.owner,
            &h.endpoint,
            (None, None),
            &fault,
            |payload, _, _, _, _| Ok(payload.capacity()),
        )
        .unwrap();
    assert!(progress.value.is_none());
    assert!(matches!(
        h.receiver.try_recv().unwrap().operation(),
        RendererBridgeOperation::Preflight { .. }
    ));
    assert!(h.queue.has_pending());
}
