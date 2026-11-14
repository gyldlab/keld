//! Real source/lease owners with modeled native commits. These tests prove
//! representation and storage custody, not native `WebMessage` delivery.
use super::*;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
use crate::original_call::request_for_lease;
use crate::renderer_bridge::NativeMessageFacts;
use crate::resource_intake::{MAX_PENDING_METADATA_BYTES, RetainedByteCredit};
use crate::resource_source::NativeNavigationUrl;
use crate::resource_source::windows::tests::Fixture;
use std::num::NonZeroU64;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Source {
    _owner: NativeDocumentOwner,
    lease: CheckedNativeDocumentLease,
    stopped: Arc<AtomicBool>,
    fixture: Fixture,
}

impl Source {
    fn new() -> Self {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let mut owner = NativeDocumentOwner::new(WebviewId(9));
        owner.bind_source(&source).unwrap();
        let attempt = owner.begin_host_load(source.source_id()).unwrap();
        let stamp = owner
            .commit_admitted(
                attempt,
                source.source_id(),
                NativeCommitIdentity(NonZeroU64::new(73).unwrap()),
            )
            .unwrap();
        let lease = CheckedNativeDocumentLease(owner.lease_for(stamp).unwrap());
        Self {
            _owner: owner,
            lease,
            stopped: Arc::new(AtomicBool::new(false)),
            fixture,
        }
    }

    fn request(&self) -> RendererBridgeRequest {
        let mut request = request_for_lease(
            self.lease.clone(),
            77,
            RendererBridgeOperation::EventQuiesced { completed: false },
            Vec::new(),
        );
        request.role_witness = Some(RendererRoleWitness::new(Arc::clone(&self.stopped)));
        request
    }
}

#[test]
fn fixed_failure_round_trip_preserves_correlation_and_actual_allocations() {
    let h = Source::new();
    for stopped in [false, true] {
        h.stopped.store(stopped, Ordering::Release);
        let mut input = Some(h.request());
        let before = Arc::strong_count(&h.stopped);
        let retained = RendererBridgeRequest::take_fixed_event_failure(&mut input).unwrap();
        assert!(input.is_none());
        assert_eq!(Arc::strong_count(&h.stopped), before);
        assert_eq!(retained.webview(), h.lease.stamp().view());
        assert_eq!(
            retained.navigation(),
            u64::from(h.lease.stamp().observed_generation())
        );
        assert_eq!(retained.request(), 77);
        assert!(retained.document_lease().same_binding(&h.lease));
        assert!(
            retained
                .role_witness()
                .same_role(&RendererRoleWitness::new(Arc::clone(&h.stopped)))
        );

        // Test-only inverse uses every owned field. Production exposes no
        // reconstruction or redispatch API for a retained failure.
        let RetainedRendererEventFailure {
            webview,
            navigation,
            request,
            document_lease,
            role_witness,
        } = retained;
        let restored = RendererBridgeRequest {
            webview,
            navigation,
            request,
            channel: 0,
            payload: Vec::new(),
            operation: RendererBridgeOperation::EventQuiesced { completed: false },
            document_lease: Some(document_lease),
            native_facts: None,
            role_witness: Some(role_witness),
            numeric_result: None,
        };
        assert!(restored.has_fixed_event_failure_metadata());
        assert_eq!(restored.webview(), h.lease.stamp().view());
        assert_eq!(
            restored.navigation(),
            u64::from(h.lease.stamp().observed_generation())
        );
        assert_eq!(restored.request(), 77);
        assert!(restored.document_lease().unwrap().same_binding(&h.lease));
        assert_eq!(restored.role_witness().unwrap().is_live(), !stopped);
        assert_eq!(Arc::strong_count(&h.stopped), before);
        drop(restored);
        assert_eq!(Arc::strong_count(&h.stopped) + 1, before);
    }
}

#[test]
fn changed_shape_returns_original_request_and_all_owned_buffers() {
    let h = Source::new();
    for case in 0..7 {
        let mut request = h.request();
        match case {
            0 => request.operation = RendererBridgeOperation::EventQuiesced { completed: true },
            1 => request.channel = 7,
            2 => request.payload = Vec::with_capacity(1),
            3 => request.payload = vec![1, 2, 3],
            4 => request.document_lease = None,
            5 => request.role_witness = None,
            6 => {
                request.operation = RendererBridgeOperation::Consumed {
                    disposition: crate::wv_link::WvConsumerDisposition::Refused {
                        code: String::from("original-code"),
                        message: String::from("original-owned-diagnostic"),
                    },
                }
            }
            _ => unreachable!(),
        }
        let payload = request.payload.as_ptr();
        let capacity = request.payload.capacity();
        let description = format!("{request:?}");
        let references = Arc::strong_count(&h.stopped);
        let mut input = Some(request);
        assert!(RendererBridgeRequest::take_fixed_event_failure(&mut input).is_none());
        let returned = input.as_ref().unwrap();
        assert_eq!(format!("{returned:?}"), description);
        assert_eq!(returned.payload.as_ptr(), payload);
        assert_eq!(returned.payload.capacity(), capacity);
        assert_eq!(Arc::strong_count(&h.stopped), references);
    }
    assert!(RendererBridgeRequest::take_fixed_event_failure(&mut None).is_none());
}

#[test]
fn actual_native_facts_and_credit_remain_with_refused_original() {
    let h = Source::new();
    let source = h.fixture.source_arc();
    let account = source.retained_credit_account();
    let baseline = account.load(Ordering::Acquire);
    let scratch = RetainedByteCredit::reserve(&account, MAX_PENDING_METADATA_BYTES).unwrap();
    let text = h.fixture.url("views/editor.html");
    let url =
        NativeNavigationUrl::capture_bounded(&text, text.encode_utf16().count(), &account).unwrap();
    let facts =
        NativeMessageFacts::from_windows_source_url(url, &source, &h.lease, &scratch).unwrap();
    drop(scratch);
    let mut original = h.request();
    original.native_facts = Some(facts);
    let allocation = original.url().unwrap().as_ptr();
    let charged = account.load(Ordering::Acquire);
    assert!(charged > baseline);
    let mut input = Some(original);
    assert!(RendererBridgeRequest::take_fixed_event_failure(&mut input).is_none());
    assert_eq!(input.as_ref().unwrap().url().unwrap().as_ptr(), allocation);
    assert_eq!(account.load(Ordering::Acquire), charged);
    drop(input);
    assert_eq!(account.load(Ordering::Acquire), baseline);
}
