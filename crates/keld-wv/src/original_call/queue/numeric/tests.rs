//! Same real retained source/lease and bounded channel; no native capture or
//! native completion is fabricated. Facts remain None in these owner controls.
use super::*;
use crate::native_document::NativeCommitIdentity;
use crate::original_binding::OriginalBinding;
use crate::original_call::queue::{OriginalCallInput, OriginalControlFault};
use crate::original_call::request_for_lease;
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::NativeAttemptObservation;
use crate::resource_source::windows::tests::Fixture;
use std::num::NonZeroU64;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

struct Harness {
    queue: OriginalCallQueue<Vec<u8>>,
    owner: NativeDocumentOwner,
    lease: CheckedNativeDocumentLease,
    binding: OriginalBinding,
    role: RendererRoleWitness,
    endpoint: SyncSender<RendererBridgeRequest>,
    receiver: mpsc::Receiver<RendererBridgeRequest>,
    fixture: Fixture,
}
impl Harness {
    fn new() -> Self {
        let fixture = Fixture::new();
        let mut owner = NativeDocumentOwner::new(WebviewId(7));
        owner.bind_source(&fixture.source_arc()).unwrap();
        let attempt = NativeAttemptObservation(
            owner
                .begin_host_load(fixture.source_arc().source_id())
                .unwrap(),
        );
        let mut binding = OriginalBinding::default();
        binding
            .prepare_attempt(&owner, attempt, "app".into(), "trusted".into())
            .unwrap();
        binding.clear_for_attempt();
        let stamp = owner
            .commit_admitted(
                attempt.0,
                fixture.source_arc().source_id(),
                NativeCommitIdentity(NonZeroU64::new(17).unwrap()),
            )
            .unwrap();
        let lease = CheckedNativeDocumentLease(owner.lease_for(stamp).unwrap());
        let candidate = binding.take_matching_commit(&owner).unwrap().unwrap();
        binding.finish_commit(&owner, candidate).unwrap();
        binding.register(&owner, "app").unwrap();
        let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
        assert!(binding.observe_route(
            &owner,
            u64::from(lease.stamp().observed_generation()),
            true,
            role.clone()
        ));
        let (endpoint, receiver) = mpsc::sync_channel(1);
        Self {
            queue: OriginalCallQueue::new_with_numeric_slot(None, WebviewId(7)).unwrap(),
            owner,
            lease,
            binding,
            role,
            endpoint,
            receiver,
            fixture,
        }
    }
    #[expect(
        clippy::unnecessary_wraps,
        reason = "Construct the actual Option slot that the tested transfer consumes or returns unchanged."
    )]
    fn request(&self, id: u32) -> Option<RendererBridgeRequest> {
        Some(request_for_lease(
            self.lease.clone(),
            id,
            RendererBridgeOperation::Invoke,
            vec![1, 2, 3],
        ))
    }
    fn begin(&mut self, id: u32) {
        let mut input = self.request(id);
        self.queue
            .begin_numeric_result(&mut input, &self.owner, None, None)
            .unwrap();
        assert!(input.is_none());
    }
    fn handoff(&mut self) -> RendererBridgeRequest {
        assert!(
            self.queue
                .flush_numeric_request(&self.owner, &self.endpoint)
                .unwrap()
        );
        self.receiver.try_recv().unwrap()
    }
    fn navigation(&self) -> u64 {
        u64::from(self.lease.stamp().observed_generation())
    }
    #[expect(
        clippy::unused_self,
        reason = "Keep the prepaid-layout query on the same fixture API used by its admission controls."
    )]
    fn fixed(&self) -> usize {
        super::super::backing::allocation_bytes::<Vec<u8>>().unwrap()
            + 64 * std::mem::size_of::<super::super::QueuedOriginalCall<Vec<u8>>>()
            + fixed_bytes().unwrap()
    }
    fn signal_address(&self) -> usize {
        Arc::as_ptr(&self.queue.numeric.as_ref().unwrap().signal) as usize
    }
}

#[test]
fn result_prepaid_before_handoff_in_same_budget_and_token_is_one_time() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<NativeNumericResultReservation>();
    let mut h = Harness::new();
    assert_eq!(h.queue.retained_bytes(), h.fixed());
    assert!(h.queue.is_drained());
    let payload_capacity = h.request(1).unwrap().payload.capacity();
    h.begin(1);
    assert_eq!(h.queue.retained_records(), 1);
    assert_eq!(
        h.queue.retained_bytes(),
        h.fixed() + record_bytes(payload_capacity).unwrap()
    );
    let mut request = h.handoff();
    let token = request.take_numeric_result().unwrap();
    assert!(request.take_numeric_result().is_none());
    assert_eq!(token.maximum_payload_len(), 4096);
    assert!(token.covers_storage(RESULT_STORAGE_BYTES));
    assert!(!token.covers_storage(RESULT_STORAGE_BYTES + 1));
    assert!(token.matches_request(WebviewId(7), h.navigation(), 1, &h.lease));
    assert!(!token.matches_request(WebviewId(8), h.navigation(), 1, &h.lease));
    assert!(!token.matches_request(WebviewId(7), h.navigation(), 2, &h.lease));
    assert!(!h.queue.is_drained());
    h.queue.cancel_numeric_result();
    drop(request);
    drop(token);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    assert!(h.queue.is_drained());
}

#[test]
fn full_retains_exact_request_token_and_all_charges() {
    let mut h = Harness::new();
    h.endpoint
        .try_send(request_for_lease(
            h.lease.clone(),
            900,
            RendererBridgeOperation::Registered,
            Vec::new(),
        ))
        .unwrap();
    h.begin(2);
    let before_bytes = h.queue.retained_bytes();
    let original = h
        .queue
        .numeric
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .outgoing
        .as_ref()
        .unwrap();
    let allocation = original.payload.as_ptr() as usize;
    assert!(
        !h.queue
            .flush_numeric_request(&h.owner, &h.endpoint)
            .unwrap()
    );
    let returned = h
        .queue
        .numeric
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .outgoing
        .as_ref()
        .unwrap();
    assert_eq!(returned.payload.as_ptr() as usize, allocation);
    assert!(
        returned
            .numeric_result
            .as_ref()
            .unwrap()
            .belongs_to(&h.queue.numeric.as_ref().unwrap().signal)
    );
    assert_eq!(h.queue.retained_bytes(), before_bytes);
    assert_eq!(h.receiver.try_recv().unwrap().request(), 900);
    let request = h.handoff();
    assert_eq!(request.request(), 2);
    assert_eq!(request.payload.as_ptr() as usize, allocation);
    h.queue.cancel_numeric_result();
    drop(request);
    assert!(h.queue.reconcile_numeric_result().unwrap());
}

struct BodyDrop {
    bytes: Vec<u8>,
    observed: Arc<AtomicUsize>,
}
impl Drop for BodyDrop {
    fn drop(&mut self) {
        self.observed.store(self.bytes.len(), Ordering::Release);
    }
}
struct OwnedResult {
    body: BodyDrop,
    storage: NativeNumericResultReservation,
}

#[test]
fn cancellation_keeps_charge_until_actual_core_thread_disposes_body_and_token() {
    let mut h = Harness::new();
    h.begin(3);
    let mut request = h.handoff();
    let token = request.take_numeric_result().unwrap();
    drop(request);
    let dropped = Arc::new(AtomicUsize::new(0));
    let held = OwnedResult {
        body: BodyDrop {
            bytes: vec![4; 4096],
            observed: Arc::clone(&dropped),
        },
        storage: token,
    };
    assert!(held.storage.covers_storage(held.body.bytes.capacity()));
    let (release, wait) = mpsc::sync_channel(0);
    let thread = std::thread::spawn(move || {
        wait.recv().unwrap();
        drop(held);
    });
    let before = h.queue.retained_bytes();
    h.queue.cancel_numeric_result();
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    assert_eq!(h.queue.retained_bytes(), before);
    assert_eq!(dropped.load(Ordering::Acquire), 0);
    let mut next = h.request(4);
    assert!(
        h.queue
            .begin_numeric_result(&mut next, &h.owner, None, None)
            .is_err()
    );
    assert!(next.is_some());
    release.send(()).unwrap();
    thread.join().unwrap();
    assert_eq!(dropped.load(Ordering::Acquire), 4096);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    assert_eq!(h.queue.retained_bytes(), h.fixed());
}

#[test]
fn actual_result_identity_and_storage_disposal_are_independent_facts() {
    let mut h = Harness::new();
    let mut foreign = Harness::new();
    h.begin(5);
    foreign.begin(5);
    let mut request = h.handoff();
    let mut other = foreign.handoff();
    let token = request.take_numeric_result().unwrap();
    let foreign_token = other.take_numeric_result().unwrap();
    assert!(
        !h.queue
            .observe_numeric_result(WebviewId(7), h.navigation(), 5, &foreign_token)
    );
    assert!(
        !h.queue
            .observe_numeric_result(WebviewId(7), h.navigation(), 6, &token)
    );
    assert!(
        h.queue
            .observe_numeric_result(WebviewId(7), h.navigation(), 5, &token)
    );
    assert!(
        !h.queue
            .observe_numeric_result(WebviewId(7), h.navigation(), 5, &token)
    );
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    drop(request);
    drop(token);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    foreign.queue.cancel_numeric_result();
    drop(other);
    drop(foreign_token);
    assert!(foreign.queue.reconcile_numeric_result().unwrap());
}

#[test]
fn storage_drop_alone_does_not_end_logical_call_or_reuse_witness() {
    let mut h = Harness::new();
    let allocation = h.signal_address();
    h.begin(7);
    drop(h.handoff());
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    assert!(h.queue.has_numeric_result());
    let mut next = h.request(8);
    assert!(
        h.queue
            .begin_numeric_result(&mut next, &h.owner, None, None)
            .is_err()
    );
    h.queue.cancel_numeric_result();
    assert!(h.queue.reconcile_numeric_result().unwrap());
    h.queue
        .begin_numeric_result(&mut next, &h.owner, None, None)
        .unwrap();
    assert_eq!(allocation, h.signal_address());
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    h.queue.cancel_numeric_result();
    let never_handed_off = h.queue.take_cancelled_numeric_request().unwrap();
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    drop(never_handed_off);
    assert!(h.queue.reconcile_numeric_result().unwrap());
}

#[test]
fn disconnected_endpoint_preserves_never_handed_off_custody_for_explicit_disposal() {
    let mut h = Harness::new();
    h.begin(9);
    let (endpoint, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    let before = h.queue.retained_bytes();
    assert!(h.queue.flush_numeric_request(&h.owner, &endpoint).is_err());
    assert_eq!(h.queue.retained_bytes(), before);
    assert!(h.queue.take_cancelled_numeric_request().is_none());
    h.queue.cancel_numeric_result();
    let original = h.queue.take_cancelled_numeric_request().unwrap();
    assert_eq!(original.request(), 9);
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    drop(original);
    assert!(h.queue.reconcile_numeric_result().unwrap());
}

mod admission;
#[test]
fn numeric_result_keeps_original_role_after_handoff_and_stop() {
    let mut h = Harness::new();
    let stop = Arc::new(AtomicBool::new(false));
    let original = RendererRoleWitness::new(Arc::clone(&stop));
    let mut request = h.request(71);
    request.as_mut().unwrap().role_witness = Some(original.clone());
    h.queue
        .begin_numeric_result(&mut request, &h.owner, None, None)
        .unwrap();
    let sent = h.handoff();
    stop.store(true, Ordering::Release);
    let (document, role) = h
        .queue
        .numeric_original(WebviewId(7), h.navigation(), 71)
        .unwrap();
    assert!(document.same_binding(&h.lease));
    assert!(role.unwrap().same_role(&original) && !role.unwrap().is_live());
    assert!(
        h.queue
            .numeric_original(WebviewId(7), h.navigation(), 72)
            .is_none()
    );
    assert!(
        h.queue
            .numeric_original(WebviewId(8), h.navigation(), 71)
            .is_none()
    );
    h.queue.cancel_numeric_result();
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    drop(sent);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    assert!(
        h.queue
            .numeric_original(WebviewId(7), h.navigation(), 71)
            .is_none()
    );
}

#[test]
fn fixed_event_failure_refuses_actual_numeric_token_without_refunding_its_account() {
    let mut h = Harness::new();
    h.begin(71);
    let mut request = h.handoff();
    // All other fixed-shape conditions hold. This actual prepaid token alone
    // must prevent the storage-only conversion from discarding numeric custody.
    request.payload = Vec::new();
    request.channel = 0;
    request.operation = RendererBridgeOperation::EventQuiesced { completed: false };
    request.role_witness = Some(h.role.clone());
    let retained = h.queue.retained_bytes();
    let references = Arc::strong_count(&h.queue.numeric.as_ref().unwrap().signal);
    let mut input = Some(request);
    assert!(RendererBridgeRequest::take_fixed_event_failure(&mut input).is_none());
    let token = input.as_ref().unwrap().numeric_result.as_ref().unwrap();
    assert!(token.matches_request(WebviewId(7), h.navigation(), 71, &h.lease));
    assert!(token.covers_storage(RESULT_STORAGE_BYTES));
    assert_eq!(h.queue.retained_bytes(), retained);
    assert_eq!(
        Arc::strong_count(&h.queue.numeric.as_ref().unwrap().signal),
        references
    );
    h.queue.cancel_numeric_result();
    assert!(!h.queue.reconcile_numeric_result().unwrap());
    drop(input);
    assert!(h.queue.reconcile_numeric_result().unwrap());
    assert!(h.queue.is_drained());
    assert_eq!(h.queue.retained_bytes(), h.fixed());
}
