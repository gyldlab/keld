//! Pure event transitions with actual retained source custody. Native commit
//! and completion inputs are modeled, not WebView2/WK acceptance evidence.

#![allow(
    clippy::expect_used,
    reason = "Test fixture prerequisites must fail immediately before exercising event custody."
)]

use super::PendingOriginalEvent;
use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner, VerifiedSourceId};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_source::windows::tests::Fixture;
use crate::wv_link::WvConsumerDisposition;
use std::num::NonZeroU64;

fn committed(
    owner: &mut NativeDocumentOwner,
    source: VerifiedSourceId,
) -> CheckedNativeDocumentLease {
    let attempt = owner.begin_host_load(source).expect("modeled Host attempt");
    let stamp = owner
        .commit_admitted(
            attempt,
            source,
            NativeCommitIdentity(NonZeroU64::new(17).expect("nonzero modeled native identity")),
        )
        .expect("modeled native commit");
    CheckedNativeDocumentLease(owner.lease_for(stamp).expect("existing owner lease"))
}

#[test]
fn admission_rechecks_living_owner_exact_lease_and_retained_source() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner.bind_source(&source).expect("real source binding");
    let lease = committed(&mut owner, source.source_id());
    let mut foreign = NativeDocumentOwner::new(WebviewId(7));
    foreign
        .bind_source(&source)
        .expect("same source, other owner");
    let other = committed(&mut foreign, source.source_id());
    assert_eq!(
        lease.stamp(),
        other.stamp(),
        "numeric coincidence is deliberate"
    );
    assert!(PendingOriginalEvent::begin(&owner, 9, other, "original".into()).is_err());
    assert!(PendingOriginalEvent::begin(&owner, 0, lease.clone(), "original".into()).is_err());
    let event = PendingOriginalEvent::begin(&owner, 9, lease.clone(), "original".into())
        .expect("same living lease admitted");
    assert_eq!(event.id(), 9);
    assert!(event.lease().same_binding(&lease));
    let mut unbound = NativeDocumentOwner::new(WebviewId(7));
    let unbound_lease = committed(&mut unbound, source.source_id());
    assert!(PendingOriginalEvent::begin(&unbound, 9, unbound_lease, "original".into()).is_err());
    owner.destroy();
    assert!(!lease.is_current());
    assert!(PendingOriginalEvent::begin(&owner, 9, lease, "original".into()).is_err());
}

#[test]
fn consumer_receipt_matches_original_once_even_after_revocation() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner.bind_source(&source).unwrap();
    let lease = committed(&mut owner, source.source_id());
    let mut event = PendingOriginalEvent::begin(&owner, 9, lease, "original".into()).unwrap();
    assert!(event.consumed_handed_off().is_err());
    assert!(
        event
            .observe_consumed(10, "original", WvConsumerDisposition::Consumed)
            .is_err()
    );
    assert!(
        event
            .observe_consumed(9, "successor", WvConsumerDisposition::Consumed)
            .is_err()
    );
    assert_eq!(event.pending_consumed(), None);
    owner.destroy();
    event
        .observe_consumed(9, "original", WvConsumerDisposition::NoListener)
        .unwrap();
    assert_eq!(
        event.pending_consumed(),
        Some(&WvConsumerDisposition::NoListener)
    );
    assert!(
        event
            .observe_consumed(9, "original", WvConsumerDisposition::Consumed)
            .is_err()
    );
    assert!(!event.consumed_sent());
    assert_eq!(event.native_quiescence(), None);
    event.consumed_handed_off().unwrap();
    assert!(event.consumed_sent());
    assert_eq!(event.pending_consumed(), None);
    assert!(event.consumed_handed_off().is_err());
    assert!(!event.retirement_controls_quiesced());
}

#[test]
fn native_receipt_requires_original_id_and_irreversible_lease() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner.bind_source(&source).unwrap();
    let lease = committed(&mut owner, source.source_id());
    let mut foreign = NativeDocumentOwner::new(WebviewId(7));
    foreign.bind_source(&source).unwrap();
    let other = committed(&mut foreign, source.source_id());
    assert_eq!(lease.stamp(), other.stamp());
    let mut event =
        PendingOriginalEvent::begin(&owner, 9, lease.clone(), "original".into()).unwrap();
    assert!(event.quiescence_handed_off().is_err());
    assert!(!event.observe_native_quiescence(10, &lease, true));
    assert!(!event.observe_native_quiescence(9, &other, true));
    assert_eq!(event.native_quiescence(), None);
    owner.destroy();
    assert!(event.observe_native_quiescence(9, &lease, false));
    assert_eq!(event.native_quiescence(), Some(false));
    assert_eq!(event.pending_quiescence(), Some(false));
    assert!(!event.quiescence_sent());
    assert_eq!(event.pending_consumed(), None);
    event.quiescence_handed_off().unwrap();
    assert!(event.quiescence_sent());
    assert_eq!(event.pending_quiescence(), None);
    assert!(event.quiescence_handed_off().is_err());
    assert!(
        event.retirement_controls_quiesced(),
        "revoked cleanup needs no invented consumer receipt"
    );
    assert!(!event.consumed_sent());
}

#[test]
fn core_release_requires_exact_generation_id_and_handed_off_native_completion() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner.bind_source(&source).unwrap();
    let lease = committed(&mut owner, source.source_id());
    let navigation = u64::from(lease.stamp().observed_generation());
    let mut event =
        PendingOriginalEvent::begin(&owner, 9, lease.clone(), "original".into()).unwrap();
    assert_eq!(event.can_release(navigation, 10), Ok(false));
    assert_eq!(event.can_release(navigation + 1, 9), Ok(false));
    assert!(event.can_release(navigation, 9).is_err());
    event
        .observe_consumed(9, "original", WvConsumerDisposition::Consumed)
        .unwrap();
    event.consumed_handed_off().unwrap();
    assert!(
        event.can_release(navigation, 9).is_err(),
        "consumer completion is not native quiescence"
    );
    assert!(event.observe_native_quiescence(9, &lease, true));
    assert!(
        event.can_release(navigation, 9).is_err(),
        "unsent native control cannot release Core custody"
    );
    assert!(!event.retirement_controls_quiesced());
    event.quiescence_handed_off().unwrap();
    assert_eq!(event.can_release(navigation, 9), Ok(true));
    assert_eq!(event.can_release(navigation, 10), Ok(false));
    assert_eq!(event.can_release(navigation + 1, 9), Ok(false));
    assert!(event.retirement_controls_quiesced());
}

#[test]
fn live_native_completion_does_not_manufacture_consumer_or_core_release() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner.bind_source(&source).unwrap();
    let lease = committed(&mut owner, source.source_id());
    let mut event =
        PendingOriginalEvent::begin(&owner, 9, lease.clone(), "original".into()).unwrap();
    assert!(event.observe_native_quiescence(9, &lease, true));
    event.quiescence_handed_off().unwrap();
    assert!(!event.retirement_controls_quiesced());
    assert_eq!(event.pending_consumed(), None);
    assert!(!event.consumed_sent());
    // The same event remains retained here; only its owning adapter may consume
    // a separately received matching Core EventReleased to remove the record.
    assert_eq!(event.id(), 9);
    owner.destroy();
    assert!(event.retirement_controls_quiesced());
    assert!(!event.consumed_sent());
}
#[test]
fn windows_event_cleanup_keeps_original_stopped_role_through_full() {
    use crate::original_binding::OriginalBinding;
    use crate::original_call::queue::OriginalCallQueue;
    use crate::renderer_bridge::{RendererBridgeOperation, RendererRoleWitness};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner.bind_source(&source).unwrap();
    let lease = committed(&mut owner, source.source_id());
    let stop = Arc::new(AtomicBool::new(false));
    let role = RendererRoleWitness::new(Arc::clone(&stop));
    let mut event = Some(
        PendingOriginalEvent::begin_windows(
            &owner,
            9,
            lease.clone(),
            "original".into(),
            role.clone(),
        )
        .unwrap(),
    );
    stop.store(true, Ordering::Release);
    owner.destroy();
    event
        .as_mut()
        .unwrap()
        .observe_native_quiescence(9, &lease, false);
    let (send, recv) = mpsc::sync_channel(1);
    send.try_send(crate::original_call::request_for_lease(
        lease.clone(),
        99,
        RendererBridgeOperation::Registered,
        Vec::new(),
    ))
    .unwrap();
    let mut queue = OriginalCallQueue::<String>::new().unwrap();
    let mut binding = OriginalBinding::default();
    queue
        .flush_controls(&mut binding, &owner, &mut event, &send)
        .unwrap();
    assert!(!event.as_ref().unwrap().quiescence_sent());
    drop(recv.try_recv().unwrap());
    queue
        .flush_controls(&mut binding, &owner, &mut event, &send)
        .unwrap();
    let request = recv.try_recv().unwrap();
    assert!(matches!(
        request.operation(),
        RendererBridgeOperation::EventQuiesced { completed: false }
    ));
    assert!(request.role_witness().unwrap().same_role(&role));
    assert!(!request.role_witness().unwrap().is_live());
    assert!(request.document_lease().unwrap().same_binding(&lease));
    assert_eq!(request.payload.capacity(), 0);
    assert!(request.native_facts.is_none() && request.numeric_result.is_none());
    assert!(!event.as_ref().unwrap().consumed_sent());
}
