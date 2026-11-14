//! Shared scheduling tests use actual retained sources and bounded channels.
//! Native commit/completion observations are modeled; no platform proof is claimed.
//! `NativeMessageFacts` is None here: Windows Source-derived nonempty facts remain
//! a separate native-adapter qualification, never a fabricated Mac-origin tuple.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "Test fixture prerequisites must fail immediately before exercising actual queue and source custody."
)]

use super::*;
use crate::WebviewId;
use crate::native_document::NativeCommitIdentity;
use crate::renderer_contract::NativeAttemptObservation;
use crate::resource_source::windows::tests::Fixture;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

struct Payload {
    bytes: String,
    drops: Arc<AtomicUsize>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::AcqRel);
    }
}
struct BodyObservation {
    allocation: usize,
    drops: Arc<AtomicUsize>,
}
impl BodyObservation {
    fn assert_held(&self) {
        assert_eq!(self.drops.load(Ordering::Acquire), 0);
    }
    fn assert_released_once(&self) {
        assert_eq!(self.drops.load(Ordering::Acquire), 1);
    }
}

struct Harness {
    queue: OriginalCallQueue<Payload>,
    binding: OriginalBinding,
    owner: NativeDocumentOwner,
    lease: CheckedNativeDocumentLease,
    fault: OriginalControlFault,
    event: Option<PendingOriginalEvent>,
    endpoint: SyncSender<RendererBridgeRequest>,
    receiver: mpsc::Receiver<RendererBridgeRequest>,
    role: RendererRoleWitness,
    role_stop: Arc<AtomicBool>,
    fixture: Rc<Fixture>,
}
impl Harness {
    fn new() -> Self {
        Self::with_fixture(Rc::new(Fixture::new()))
    }
    fn sharing_source(&self) -> Self {
        Self::with_fixture(Rc::clone(&self.fixture))
    }
    fn with_fixture(fixture: Rc<Fixture>) -> Self {
        let mut owner = NativeDocumentOwner::new(WebviewId(7));
        owner.bind_source(&fixture.source_arc()).unwrap();
        let mut binding = OriginalBinding::default();
        let attempt = NativeAttemptObservation(
            owner
                .begin_host_load(fixture.source_arc().source_id())
                .unwrap(),
        );
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
        let (endpoint, receiver) = mpsc::sync_channel(1);
        let role_stop = Arc::new(AtomicBool::new(false));
        let role = RendererRoleWitness::new(Arc::clone(&role_stop));
        assert!(binding.observe_route(
            &owner,
            u64::from(lease.stamp().observed_generation()),
            true,
            role.clone()
        ));
        let mut harness = Self {
            queue: OriginalCallQueue::new().unwrap(),
            binding,
            owner,
            lease,
            fault: OriginalControlFault::default(),
            event: None,
            endpoint,
            receiver,
            role,
            role_stop,
            fixture,
        };
        harness.flush();
        assert!(matches!(
            harness.receive().operation(),
            RendererBridgeOperation::Registered
        ));
        harness
    }
    fn enqueue(&mut self, id: u32, effect: bool) -> BodyObservation {
        let drops = Arc::new(AtomicUsize::new(0));
        let payload = Payload {
            bytes: format!("retained original invocation {id}"),
            drops: Arc::clone(&drops),
        };
        let observed = BodyObservation {
            allocation: payload.bytes.as_ptr() as usize,
            drops,
        };
        self.queue
            .enqueue(
                OriginalCallInput {
                    id,
                    lease: self.lease.clone(),
                    payload_bytes: payload.bytes.capacity(),
                    payload,
                    credential: "app".into(),
                    channel: effect.then(|| "rendererReq".into()),
                    role_witness: effect.then(|| self.role.clone()),
                    facts: None,
                },
                &self.binding,
                &self.owner,
                &self.endpoint,
                None,
                &mut self.fault,
            )
            .unwrap();
        observed
    }
    fn flush(&mut self) {
        self.queue
            .flush_controls(
                &mut self.binding,
                &self.owner,
                &mut self.event,
                &self.endpoint,
            )
            .unwrap();
    }
    fn take(&mut self) -> Option<PreparingOriginalInvocation<Payload>> {
        self.take_progress().value
    }
    fn take_progress(
        &mut self,
    ) -> OriginalQueueProgress<Option<PreparingOriginalInvocation<Payload>>> {
        self.queue
            .take_next_preparation(
                &self.binding,
                &self.owner,
                &self.endpoint,
                (None, self.event.as_ref()),
                &self.fault,
                |payload, _, _, _, limit| {
                    assert!(payload.bytes.capacity() <= limit);
                    Ok(payload.bytes.capacity())
                },
            )
            .unwrap()
    }
    fn enter(
        &mut self,
        token: &mut PreparingOriginalInvocation<Payload>,
        kind: OriginalInvocationKind,
    ) -> bool {
        if token.kind() != kind {
            return false;
        }
        self.queue.try_enter_prepared(
            token,
            &self.binding,
            &self.owner,
            (None, self.event.as_ref()),
            &self.fault,
        )
    }
    fn finish(&mut self, id: u32) {
        let active = self.queue.active_mut().expect("same entered record");
        active.observe_function_finished(id, "app").unwrap();
        assert!(active.observe_native_quiescence(id, &self.lease, true));
    }
    fn receive(&self) -> RendererBridgeRequest {
        self.receiver.try_recv().unwrap()
    }
    fn assert_endpoint_empty(&self) {
        assert!(matches!(
            self.receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    fn fill_endpoint(&self) {
        self.endpoint
            .try_send(request_for_lease(
                self.lease.clone(),
                900,
                RendererBridgeOperation::Registered,
                Vec::new(),
            ))
            .unwrap();
    }
    fn accept_preflight(&mut self, id: u32) {
        self.queue
            .deliver_preflight(
                u64::from(self.lease.stamp().observed_generation()),
                id,
                true,
                &self.binding,
                &self.owner,
            )
            .unwrap();
    }
}

fn allocation(token: &mut PreparingOriginalInvocation<Payload>) -> usize {
    token.payload().unwrap().bytes.as_ptr() as usize
}

#[path = "queue/tests/accounting.rs"]
mod accounting;
#[path = "queue/tests/fault.rs"]
mod fault;
#[path = "queue/tests/ordering.rs"]
mod ordering;
#[path = "queue/tests/preparing.rs"]
mod preparing;

#[path = "queue_tests/cold_storage.rs"]
mod cold_storage;

#[path = "queue_tests/input.rs"]
mod input;
