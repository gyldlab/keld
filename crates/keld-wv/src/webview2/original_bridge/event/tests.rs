//! Real retained files/leases with modeled native phase inputs; no COM proof.
use super::*;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
use crate::renderer_bridge::NativeDataCharge;
use crate::resource_source::windows::tests::Fixture;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Charge(Arc<AtomicBool>);
impl NativeDataCharge for Charge {
    fn try_extend_storage(&self, _encoded: usize, _additional: usize) -> bool {
        true
    }
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn event() -> (
    Fixture,
    NativeDocumentOwner,
    NativeEvent,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
) {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(71));
    owner.bind_source(&source).unwrap();
    let attempt = owner.begin_host_load(source.source_id()).unwrap();
    let stamp = owner
        .commit_admitted(
            attempt,
            source.source_id(),
            NativeCommitIdentity(std::num::NonZeroU64::new(17).unwrap()),
        )
        .unwrap();
    let document = CheckedNativeDocumentLease(owner.lease_for(stamp).unwrap());
    let stopped = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let mut outcome = Some(RendererBridgeOutcome::Event {
        webview: WebviewId(71),
        navigation: u64::from(document.stamp().observed_generation()),
        request: 9,
        channel: String::from("mainResp"),
        payload: vec![1, 2, 3],
        document,
        role: RendererRoleWitness::new(Arc::clone(&stopped)),
        storage: Some(NativeDataReservation::from_owner(Charge(Arc::clone(
            &dropped,
        )))),
    });
    let event = NativeEvent::attach(&mut outcome, WebviewId(71), &source)
        .unwrap()
        .unwrap();
    assert!(outcome.is_none());
    (fixture, owner, event, stopped, dropped)
}

#[test]
fn never_entered_retirement_irreversibly_cancels_same_operation_before_proof() {
    let (_fixture, mut owner, mut event, _stopped, dropped) = event();
    assert!(
        event.select_retirement(false).is_none(),
        "live Prepared is not retirement"
    );
    owner.destroy();
    let proof = event.select_retirement(false).unwrap();
    assert_eq!(proof.certainty(), WvDispatchCertainty::NotDispatched);
    assert_eq!(proof.delivery(), 9);
    assert!(proof.document().same_binding(event.document()));
    assert!(proof.role().same_role(event.role()));
    assert_eq!(event.phase, PostPhase::Cancelled);
    assert!(event.prepare("0123456789abcdef0123456789abcdef").is_err());
    assert!(matches!(
        event.phase.prior_attempt_result(),
        Some(PostResult::NotAdmitted)
    ));
    assert!(!dropped.load(Ordering::Acquire));
    assert!(
        !event
            .observe_release(
                u64::from(event.document.stamp().observed_generation()),
                9,
                None
            )
            .unwrap()
    );
    event.retirement_handed_off().unwrap();
    assert!(
        event.select_retirement(false).is_none(),
        "no duplicate handoff"
    );
    assert!(
        event
            .observe_release(
                u64::from(event.document.stamp().observed_generation()),
                9,
                None
            )
            .unwrap()
    );
    assert!(
        event.disposable(false),
        "cancellation proves never-entered body"
    );
    drop(event);
    assert!(dropped.load(Ordering::Acquire));
}

#[test]
fn entered_retirement_needs_actual_close_and_core_release_does_not_end_in_call() {
    let (_fixture, mut owner, mut event, _stopped, dropped) = event();
    owner.destroy();
    event.phase = PostPhase::Posting; // Pure physical-state fault control only.
    assert!(event.select_retirement(false).is_none());
    let proof = event.select_retirement(true).unwrap(); // Modeled Close input, not COM.
    assert_eq!(proof.certainty(), WvDispatchCertainty::OutcomeUnknown);
    assert!(!event.phase.cancel_unentered());
    event.retirement_handed_off().unwrap();
    assert!(
        event
            .observe_release(
                u64::from(event.document.stamp().observed_generation()),
                9,
                None
            )
            .unwrap()
    );
    assert!(
        !event.disposable(true),
        "reentrant Close cannot free in-call bytes"
    );
    assert!(!dropped.load(Ordering::Acquire));
    event.phase = PostPhase::Returned; // Modeled actual return, independent of release.
    assert!(event.disposable(true));
    drop(event);
    assert!(dropped.load(Ordering::Acquire));
}

#[test]
fn post_return_or_stopped_role_cannot_replace_entered_close_proof() {
    let (_fixture, _owner, mut event, stopped, _dropped) = event();
    stopped.store(true, Ordering::Release);
    event.phase = PostPhase::Returned;
    assert!(event.select_retirement(false).is_none());
    event.phase = PostPhase::OutcomeUnknown;
    assert!(event.select_retirement(false).is_none());
    assert!(
        !event.disposable(true),
        "actual Core terminal remains separate"
    );
    let proof = event.select_retirement(true).unwrap();
    assert_eq!(proof.certainty(), WvDispatchCertainty::OutcomeUnknown);
    assert!(
        proof.role().same_role(&event.role),
        "cleanup preserves stopped original"
    );
    assert!(!proof.role().is_live());
}

#[test]
fn failed_attachment_keeps_exact_original_outcome_and_storage_token() {
    let (_fixture, _owner, event, _stopped, dropped) = event();
    let source = Fixture::new();
    let view = event.document.stamp().view();
    let pointer = event.payload.as_ptr();
    let NativeEvent {
        delivery,
        document,
        role,
        channel,
        payload,
        storage,
        ..
    } = event;
    let mut pending = Some(RendererBridgeOutcome::Event {
        webview: view,
        navigation: u64::from(document.stamp().observed_generation()),
        request: delivery,
        channel,
        payload,
        document,
        role,
        storage: Some(storage),
    });
    assert!(NativeEvent::attach(&mut pending, view, &source.source_arc()).is_err());
    let Some(RendererBridgeOutcome::Event { payload, .. }) = &pending else {
        panic!("same outcome")
    };
    assert_eq!(payload.as_ptr(), pointer);
    assert!(!dropped.load(Ordering::Acquire));
    drop(pending);
    assert!(dropped.load(Ordering::Acquire));
}

#[test]
fn event_encoding_uses_data_growth_and_exact_canonical_carrier() {
    struct Counted(Arc<AtomicUsize>);
    impl NativeDataCharge for Counted {
        fn try_extend_storage(&self, encoded: usize, additional: usize) -> bool {
            assert_eq!(additional, encoded + 2 * (encoded + 1));
            self.0.fetch_add(1, Ordering::Relaxed);
            true
        }
    }
    let (_fixture, _owner, mut event, _stopped, _dropped) = event();
    let growth = Arc::new(AtomicUsize::new(0));
    event.storage = NativeDataReservation::from_owner(Counted(Arc::clone(&growth)));
    event.prepare("0123456789abcdef0123456789abcdef").unwrap();
    let golden = r#"{"v":2,"document":"0123456789abcdef0123456789abcdef","kind":"event","delivery":9,"channel":"mainResp","payload":[1,2,3]}"#;
    let mut expected: Vec<u16> = golden.encode_utf16().collect();
    expected.push(0);
    assert_eq!(event.wide, expected);
    assert_eq!(growth.load(Ordering::Relaxed), 1);
    event.prepare("0123456789abcdef0123456789abcdef").unwrap();
    assert_eq!(
        growth.load(Ordering::Relaxed),
        1,
        "same carrier is not recharged/reencoded"
    );
    assert!(size_of::<NativeEvent>() + size_of::<NativeEventRetirement>() <= 1024);
}

#[test]
fn earlier_actual_event_release_survives_a_later_full_retirement_control() {
    let (_fixture, mut owner, mut event, _stopped, dropped) = event();
    let mut pending = crate::original_event::PendingOriginalEvent::begin(
        &owner,
        event.delivery,
        event.document.clone(),
        String::from("original"),
    )
    .unwrap();
    assert!(pending.observe_native_quiescence(event.delivery, &event.document, true));
    pending.quiescence_handed_off().unwrap();
    event.phase = PostPhase::Returned; // Modeled actual API return.
    owner.destroy();
    let _not_handed_off = event.select_retirement(true).unwrap();
    assert!(!event.retirement_sent);
    assert!(
        event
            .observe_release(
                u64::from(event.document.stamp().observed_generation()),
                event.delivery,
                Some(&pending),
            )
            .unwrap(),
        "prior actual Core terminal is not replaced by later cancellation"
    );
    assert!(
        event.disposable(false),
        "physical return was already observed"
    );
    assert!(!dropped.load(Ordering::Acquire));
    drop(event);
    assert!(dropped.load(Ordering::Acquire));
}

#[test]
fn stopped_role_cannot_certify_unentered_event_while_exact_lease_is_current() {
    let (_fixture, mut owner, mut event, stopped, dropped) = event();
    stopped.store(true, Ordering::Release);
    assert!(event.document.is_current());
    assert!(event.select_retirement(false).is_none());
    assert_eq!(
        event.phase,
        PostPhase::Cancelled,
        "same operation is irreversibly blocked"
    );
    assert!(
        event.retirement.is_none(),
        "stopped role is not lease revocation"
    );
    assert!(
        event.select_retirement(true).is_none(),
        "even Close is not this lease's revoke fact"
    );
    assert!(!dropped.load(Ordering::Acquire));
    owner.destroy();
    let proof = event.select_retirement(false).unwrap();
    assert_eq!(proof.certainty(), WvDispatchCertainty::NotDispatched);
    assert!(proof.document().same_binding(event.document()));
    assert!(!proof.document().is_current());
    assert!(
        !dropped.load(Ordering::Acquire),
        "logical proof still retains physical storage"
    );
}

#[test]
fn prepared_event_keeps_its_actual_protocol_without_entered_receipts() {
    let (_fixture, owner, mut event, stopped, dropped) = event();
    let epoch = "0123456789abcdef0123456789abcdef";
    event.prepare_protocol(&owner, epoch).unwrap();
    let prepared = event.prepared_record().expect("same prepared protocol");
    assert_eq!(prepared.id(), event.delivery());
    assert!(prepared.lease().same_binding(event.document()));
    assert!(prepared.original_role().unwrap().same_role(event.role()));
    assert!(prepared.native_quiescence().is_none());
    assert!(prepared.pending_consumed().is_none());
    assert_eq!(event.phase, PostPhase::Prepared);
    stopped.store(true, Ordering::Release);
    assert!(
        event.select_retirement(false).is_none(),
        "current lease cannot retire by role stop"
    );
    assert_eq!(event.phase, PostPhase::Cancelled);
    assert!(
        event.prepared_record().is_some(),
        "logical cancellation retains original metadata"
    );
    assert!(
        !event.disposable(false),
        "teardown is not a production release receipt"
    );
    assert!(!dropped.load(Ordering::Acquire));
    drop(event);
    assert!(dropped.load(Ordering::Acquire));
}
