use super::*;

#[test]
fn preparation_borrows_the_same_backing_and_is_not_a_receipt_target() {
    let mut h = Harness::new();
    let body = h.enqueue(1, false);
    let before = h.queue.retained_bytes();
    let mut token = h.take().unwrap();
    assert_eq!(allocation(&mut token), body.allocation);
    assert!(h.queue.has_active());
    assert!(
        h.queue.active().is_none(),
        "Preparing exposes no Function/native receipt target"
    );
    assert!(
        h.take().is_none(),
        "the same backing cannot issue a second handle"
    );
    assert!(h.queue.clear_entered().is_err());
    assert_eq!(
        (h.queue.retained_records(), h.queue.retained_bytes()),
        (1, before)
    );
    body.assert_held();

    h.queue.defer_preparation(token).unwrap();
    let mut deferred = h.take().unwrap();
    assert_eq!(allocation(&mut deferred), body.allocation);
    assert!(h.enter(&mut deferred, OriginalInvocationKind::Original));
    deferred.native_returned().unwrap();
    assert!(
        !h.queue.active().unwrap().ready_to_release(),
        "API return creates no completion"
    );
    h.finish(1);
    h.flush();
    assert!(h.queue.is_drained());
    body.assert_released_once();
}

#[test]
fn inline_function_and_native_completion_wait_for_actual_api_return() {
    let mut h = Harness::new();
    let body = h.enqueue(2, false);
    let charge = h.queue.retained_bytes();
    let mut token = h.take().unwrap();
    assert!(h.enter(&mut token, OriginalInvocationKind::Original));
    h.finish(2); // Modeled inline completion while the native API is still entered.
    assert!(h.queue.active().unwrap().ready_to_release());
    h.flush();
    assert!(h.queue.has_active());
    assert!(h.queue.clear_entered().is_err());
    assert_eq!(h.queue.retained_bytes(), charge);
    body.assert_held();
    assert!(h.take().is_none());

    token.native_returned().unwrap();
    h.flush();
    assert!(h.queue.is_drained());
    body.assert_released_once();
}

#[test]
fn same_numeric_id_and_source_cannot_exchange_two_queues_backings() {
    let mut a = Harness::new();
    let mut b = a.sharing_source();
    let body_a = a.enqueue(3, false);
    let body_b = b.enqueue(3, false);
    assert_eq!(a.lease.stamp(), b.lease.stamp());
    assert!(!a.lease.same_binding(&b.lease));
    let mut token_a = a.take().unwrap();
    let mut token_b = b.take().unwrap();
    assert!(
        !a.queue
            .try_enter_prepared(&token_b, &a.binding, &a.owner, (None, None), &a.fault)
    );
    assert!(
        !b.queue
            .try_enter_prepared(&token_a, &b.binding, &b.owner, (None, None), &b.fault)
    );
    assert_eq!(allocation(&mut token_a), body_a.allocation);
    assert_eq!(allocation(&mut token_b), body_b.allocation);
    body_a.assert_held();
    body_b.assert_held();
    assert!(a.enter(&mut token_a, OriginalInvocationKind::Original));
    assert!(b.enter(&mut token_b, OriginalInvocationKind::Original));
    a.finish(3);
    b.finish(3);
    token_a.native_returned().unwrap();
    a.flush();
    b.flush();
    body_a.assert_released_once();
    body_b.assert_held();
    token_b.native_returned().unwrap();
    b.flush();
    body_b.assert_released_once();
}

#[test]
fn revocation_during_preparation_waits_for_explicit_no_entry_and_handle_release() {
    let mut h = Harness::new();
    let body = h.enqueue(4, false);
    let mut token = h.take().unwrap();
    h.owner.destroy();
    h.queue.cancel_queued().unwrap();
    h.flush();
    assert!(!h.enter(&mut token, OriginalInvocationKind::Original));
    assert!(h.queue.active().is_none());
    assert_eq!(h.queue.retained_records(), 1);
    body.assert_held();
    h.queue.defer_preparation(token).unwrap();
    h.flush();
    assert!(h.queue.is_drained());
    body.assert_released_once();
    h.assert_endpoint_empty();
}

#[test]
fn final_entry_rechecks_actual_event_and_send_records_then_living_owner() {
    let mut h = Harness::new();
    let body = h.enqueue(5, true);
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::Preflight { .. }
    ));
    h.accept_preflight(5);
    let mut token = h.take().unwrap();
    h.event =
        Some(PendingOriginalEvent::begin(&h.owner, 80, h.lease.clone(), "app".into()).unwrap());
    assert!(!h.enter(&mut token, OriginalInvocationKind::Original));
    {
        let event = h.event.as_mut().unwrap();
        event
            .observe_consumed(80, "app", crate::wv_link::WvConsumerDisposition::Consumed)
            .unwrap();
        assert!(event.observe_native_quiescence(80, &h.lease, true));
    }
    h.flush();
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::Consumed { .. }
    ));
    h.flush();
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::EventQuiesced { .. }
    ));
    assert!(
        h.event
            .as_ref()
            .unwrap()
            .can_release(u64::from(h.lease.stamp().observed_generation()), 80)
            .unwrap()
    );
    h.event = None; // Modeled exact Core EventReleased after actual handoffs.
    h.endpoint
        .try_send(request_for_lease(
            h.lease.clone(),
            81,
            RendererBridgeOperation::Send {
                channel: "rendererReq".into(),
                original_call: 5,
            },
            Vec::new(),
        ))
        .unwrap();
    let send = PendingOriginalSend::after_handoff(81, h.lease.clone());
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::Send { .. }
    ));
    assert!(!h.queue.try_enter_prepared(
        &token,
        &h.binding,
        &h.owner,
        (Some(&send), None),
        &h.fault
    ));
    assert!(send.matches_result(u64::from(h.lease.stamp().observed_generation()), 81));
    drop(send); // Modeled exact Core terminal outcome for the accepted Send.
    let other = h.sharing_source();
    assert!(
        !h.queue
            .try_enter_prepared(&token, &h.binding, &other.owner, (None, None), &h.fault)
    );
    h.queue.defer_preparation(token).unwrap();
    h.flush();
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::PreflightAborted
    ));
    let mut refusal = h.take().unwrap();
    assert!(h.enter(&mut refusal, OriginalInvocationKind::Refusal));
    refusal.native_returned().unwrap();
    assert!(
        h.queue
            .active_mut()
            .unwrap()
            .observe_native_quiescence(5, &h.lease, true)
    );
    h.flush();
    body.assert_released_once();
}

#[test]
fn returned_api_with_an_extra_immutable_owner_cannot_release_or_reuse_backing() {
    let mut h = Harness::new();
    let body = h.enqueue(6, false);
    let mut token = h.take().unwrap();
    // Test-only additional strong owner; production exposes no Rc clone API.
    let held = Rc::clone(&token.backing);
    assert!(h.enter(&mut token, OriginalInvocationKind::Original));
    h.finish(6);
    token.native_returned().unwrap();
    h.flush();
    assert!(h.queue.has_active());
    assert!(h.take().is_none());
    body.assert_held();
    drop(held);
    h.flush();
    body.assert_released_once();
    assert!(h.queue.is_drained());
}

#[test]
fn deferred_refusal_requests_one_transition_wake_but_occupied_effect_does_not() {
    let mut h = Harness::new();
    let body = h.enqueue(7, true);
    h.receive();
    h.accept_preflight(7);
    let mut token = h.take().unwrap();
    h.event =
        Some(PendingOriginalEvent::begin(&h.owner, 82, h.lease.clone(), "app".into()).unwrap());
    assert!(!h.enter(&mut token, OriginalInvocationKind::Original));
    let defer = h.queue.defer_preparation(token).unwrap();
    assert!(
        !defer.needs_service_wake,
        "waiting for the same event is not a wake transition"
    );
    let mut token = h.take().unwrap();
    h.role_stop.store(true, Ordering::Release);
    assert!(!h.enter(&mut token, OriginalInvocationKind::Original));
    let controls = h
        .queue
        .flush_controls(&mut h.binding, &h.owner, &mut h.event, &h.endpoint)
        .unwrap();
    assert!(
        controls.needs_service_wake,
        "actual preflight abort makes refusal eligible"
    );
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::PreflightAborted
    ));
    let defer = h.queue.defer_preparation(token).unwrap();
    assert!(
        defer.needs_service_wake,
        "same borrowed refusal became available only on this defer"
    );
    let mut refusal = h.take().unwrap();
    assert!(h.enter(&mut refusal, OriginalInvocationKind::Refusal));
    refusal.native_returned().unwrap();
    assert!(
        h.queue
            .active_mut()
            .unwrap()
            .observe_native_quiescence(7, &h.lease, true)
    );
    h.flush();
    body.assert_released_once();
}

#[test]
fn foreign_defer_error_keeps_both_payloads_in_their_original_owners() {
    let mut a = Harness::new();
    let mut b = a.sharing_source();
    let body_a = a.enqueue(8, false);
    let body_b = b.enqueue(8, false);
    let mut token_a = a.take().unwrap();
    let token_b = b.take().unwrap();
    let charge_b = b.queue.retained_bytes();
    assert!(a.queue.defer_preparation(token_b).is_err());
    assert_eq!(a.queue.backing.phase.get(), PreparationPhase::Borrowed);
    assert_eq!(b.queue.backing.phase.get(), PreparationPhase::Unproved);
    body_a.assert_held();
    body_b.assert_held();
    assert!(a.enter(&mut token_a, OriginalInvocationKind::Original));
    token_a.native_returned().unwrap();
    a.finish(8);
    a.flush();
    body_a.assert_released_once();
    b.flush();
    assert_eq!(b.queue.retained_bytes(), charge_b);
    assert!(!b.queue.is_drained());
    body_b.assert_held();
    drop(b); // Pure retained-owner lifetime, not a native release receipt.
    body_b.assert_released_once();
}
