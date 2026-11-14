use super::*;
use crate::wv_link::WvConsumerDisposition;

#[test]
fn returned_local_stays_before_a_preflight_with_no_remaining_queued_predecessors() {
    let mut h = Harness::new();
    let a = h.enqueue(10, false);
    let token = h.take().unwrap();
    let b = h.enqueue(11, true);
    let request = h.receive();
    assert_eq!(request.request(), 11);
    assert!(matches!(
        request.operation(),
        RendererBridgeOperation::Preflight { .. }
    ));
    assert_eq!(h.queue.pending_observation().unwrap().2, 0);
    h.accept_preflight(11);
    h.queue.defer_preparation(token).unwrap();
    let mut token = h.take().unwrap();
    assert_eq!(token.identity().id(), 10);
    assert_eq!(allocation(&mut token), a.allocation);
    assert!(h.enter(&mut token, OriginalInvocationKind::Original));
    token.native_returned().unwrap();
    h.finish(10);
    h.flush();
    a.assert_released_once();
    b.assert_held();

    let mut token = h.take().unwrap();
    assert_eq!(token.identity().id(), 11);
    assert!(h.enter(&mut token, OriginalInvocationKind::Original));
    token.native_returned().unwrap();
    h.finish(11);
    h.flush();
    let abort = h.receive();
    assert_eq!(abort.request(), 11);
    assert!(matches!(
        abort.operation(),
        RendererBridgeOperation::PreflightAborted
    ));
    assert!(h.queue.is_drained());
    b.assert_released_once();
}

#[test]
fn full_preflight_preserves_payload_order_charge_and_original_role() {
    let mut h = Harness::new();
    h.fill_endpoint();
    let body = h.enqueue(20, true);
    let local = h.enqueue(21, false);
    let charge = h.queue.retained_bytes();
    let full = h.take_progress();
    assert!(full.value.is_none());
    assert!(
        !full.needs_service_wake,
        "Full alone cannot create a service spin"
    );
    assert_eq!(h.queue.local_len(), 2);
    assert_eq!(h.queue.retained_bytes(), charge);
    body.assert_held();
    h.receive(); // Release the real bounded channel's occupied slot.
    assert!(
        h.take().is_none(),
        "preflight handoff is not Function entry"
    );
    let request = h.receive();
    assert_eq!(request.request(), 20);
    assert!(request.document_lease().unwrap().same_binding(&h.lease));
    assert!(request.role_witness().unwrap().same_role(&h.role));
    assert!(
        request.url().is_none(),
        "nonempty Windows native facts are not fabricated"
    );
    h.accept_preflight(20);

    h.role_stop.store(true, Ordering::Release);
    let successor = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    assert!(h.binding.observe_route(
        &h.owner,
        u64::from(h.lease.stamp().observed_generation()),
        true,
        successor
    ));
    h.flush();
    assert!(matches!(
        h.receive().operation(),
        RendererBridgeOperation::PreflightAborted
    ));
    assert_eq!(h.queue.retained_bytes(), charge);
    let mut refused = h.take().unwrap();
    assert_eq!(refused.identity().id(), 20);
    assert_eq!(allocation(&mut refused), body.allocation);
    assert!(!h.enter(&mut refused, OriginalInvocationKind::Original));
    assert!(h.enter(&mut refused, OriginalInvocationKind::Refusal));
    refused.native_returned().unwrap();
    assert!(
        h.queue
            .active_mut()
            .unwrap()
            .observe_native_quiescence(20, &h.lease, true)
    );
    h.flush();
    body.assert_released_once();

    let mut next = h.take().unwrap();
    assert_eq!(next.identity().id(), 21);
    assert!(
        h.enter(&mut next, OriginalInvocationKind::Original),
        "local remains role-independent"
    );
    next.native_returned().unwrap();
    h.finish(21);
    h.flush();
    local.assert_released_once();
    assert!(h.queue.is_drained());
}

#[test]
fn role_change_during_preparation_waits_for_original_abort_before_refusal() {
    let mut h = Harness::new();
    let body = h.enqueue(30, true);
    h.receive();
    h.accept_preflight(30);
    let mut token = h.take().unwrap();
    let charge = h.queue.retained_bytes();
    h.fill_endpoint();
    h.role_stop.store(true, Ordering::Release);
    assert!(!h.enter(&mut token, OriginalInvocationKind::Original));
    h.flush();
    assert_eq!(token.kind(), OriginalInvocationKind::Original);
    assert!(!h.enter(&mut token, OriginalInvocationKind::Refusal));
    h.queue.defer_preparation(token).unwrap();
    assert!(h.take().is_none(), "Full abort cannot become refusal entry");
    assert_eq!(h.queue.retained_bytes(), charge);
    body.assert_held();

    h.receive();
    h.flush();
    let abort = h.receive();
    assert_eq!(abort.request(), 30);
    assert!(matches!(
        abort.operation(),
        RendererBridgeOperation::PreflightAborted
    ));
    let mut token = h.take().unwrap();
    assert_eq!(token.kind(), OriginalInvocationKind::Refusal);
    assert!(h.enter(&mut token, OriginalInvocationKind::Refusal));
    token.native_returned().unwrap();
    h.flush();
    assert!(
        h.queue.has_active(),
        "refusal dispatch/return is not native completion"
    );
    body.assert_held();
    assert!(
        h.queue
            .active_mut()
            .unwrap()
            .observe_native_quiescence(30, &h.lease, true)
    );
    h.flush();
    body.assert_released_once();
}

#[test]
fn full_controls_keep_consumed_quiesced_and_abort_on_their_original_records() {
    let mut h = Harness::new();
    let body = h.enqueue(40, true);
    h.receive();
    let mut event =
        PendingOriginalEvent::begin(&h.owner, 90, h.lease.clone(), "app".into()).unwrap();
    event
        .observe_consumed(90, "app", WvConsumerDisposition::Consumed)
        .unwrap();
    assert!(event.observe_native_quiescence(90, &h.lease, true));
    h.event = Some(event);
    h.role_stop.store(true, Ordering::Release);
    h.flush();
    let consumed = h.receive();
    assert_eq!(consumed.request(), 90);
    assert!(matches!(
        consumed.operation(),
        RendererBridgeOperation::Consumed { .. }
    ));
    assert!(h.event.as_ref().unwrap().consumed_sent());
    assert!(!h.event.as_ref().unwrap().quiescence_sent());
    assert!(h.queue.has_pending());

    h.flush();
    let quiesced = h.receive();
    assert_eq!(quiesced.request(), 90);
    assert!(matches!(
        quiesced.operation(),
        RendererBridgeOperation::EventQuiesced { completed: true }
    ));
    assert!(h.queue.has_pending());
    h.flush();
    let abort = h.receive();
    assert_eq!(abort.request(), 40);
    assert!(matches!(
        abort.operation(),
        RendererBridgeOperation::PreflightAborted
    ));
    assert!(!h.queue.has_pending());
    assert!(
        h.event
            .as_ref()
            .unwrap()
            .can_release(u64::from(h.lease.stamp().observed_generation()), 90)
            .unwrap()
    );
    // Modeled actual Core EventReleased after both native controls were handed off.
    h.event = None;
    let mut token = h.take().unwrap();
    assert!(h.enter(&mut token, OriginalInvocationKind::Refusal));
    token.native_returned().unwrap();
    assert!(
        h.queue
            .active_mut()
            .unwrap()
            .observe_native_quiescence(40, &h.lease, true)
    );
    h.flush();
    body.assert_released_once();
}

#[test]
fn late_preflight_outcomes_cannot_admit_a_different_original_call() {
    let mut h = Harness::new();
    let body = h.enqueue(50, true);
    h.receive();
    let navigation = u64::from(h.lease.stamp().observed_generation());
    for (generation, id) in [(navigation + 1, 50), (navigation, 51)] {
        h.queue
            .deliver_preflight(generation, id, true, &h.binding, &h.owner)
            .unwrap();
        assert_eq!(
            h.queue.pending_observation().unwrap().3,
            OriginalCallAdmission::Waiting
        );
        assert!(h.take().is_none());
    }
    h.accept_preflight(50);
    let mut token = h.take().unwrap();
    assert!(h.enter(&mut token, OriginalInvocationKind::Original));
    token.native_returned().unwrap();
    h.finish(50);
    h.flush();
    assert_eq!(h.receive().request(), 50);
    body.assert_released_once();
}

#[test]
fn role_change_before_preflight_handoff_requests_the_existing_refusal_service_wake() {
    let mut h = Harness::new();
    h.fill_endpoint();
    let body = h.enqueue(51, true);
    h.receive(); // No preflight ever entered Core; the existing endpoint is empty.
    let queued = h.queue.local_calls.pop_front().unwrap();
    let charge = h.queue.retained_bytes();
    // Deterministic second-check cut of the actual existing preflight method:
    // the captured role retires after selection and before any native/Host handoff.
    h.role_stop.store(true, Ordering::Release);
    let step = h
        .queue
        .begin_preflight(queued, 0, &h.binding, &h.owner, &h.endpoint)
        .unwrap();
    assert!(
        step.needs_service_wake,
        "without this transition wake no handoff can resume service"
    );
    let queued = step.value.unwrap();
    assert!(queued.cancelled);
    h.queue.local_calls.push_front(queued); // Same caller-owned record, same queue position.
    h.assert_endpoint_empty();
    assert!(!h.queue.has_pending());
    assert_eq!(h.queue.retained_bytes(), charge);
    body.assert_held();

    // Model only delivery of that existing service notification, not native I/O.
    let mut refusal = h.take().unwrap();
    assert_eq!(refusal.kind(), OriginalInvocationKind::Refusal);
    assert!(!h.enter(&mut refusal, OriginalInvocationKind::Original));
    assert!(h.enter(&mut refusal, OriginalInvocationKind::Refusal));
    refusal.native_returned().unwrap();
    assert!(
        h.queue
            .active_mut()
            .unwrap()
            .observe_native_quiescence(51, &h.lease, true)
    );
    h.flush();
    body.assert_released_once();
    h.assert_endpoint_empty();
}
