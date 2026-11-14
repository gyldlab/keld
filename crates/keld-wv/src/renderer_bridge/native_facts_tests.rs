//! Real retained-source/credit component controls with modeled native commit.
//! No `WebMessageReceived` callback or native `SecurityOrigin` is fabricated here.

#![allow(
    clippy::unwrap_used,
    reason = "Test fixture prerequisites must fail immediately before exercising real source and credit custody."
)]

use super::*;
use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
use crate::renderer_bridge::{RendererBridgeOperation, RendererBridgeRequest};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_intake::{MAX_PENDING_METADATA_BYTES, RetainedByteCredit};
use crate::resource_source::AdmittedWebAppSource;
use crate::resource_source::windows::tests::Fixture;
use std::num::NonZeroU64;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct BoundSource {
    owner: NativeDocumentOwner,
    source: Arc<AdmittedWebAppSource>,
    lease: CheckedNativeDocumentLease,
    fixture: Fixture,
}
impl BoundSource {
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
            owner,
            source,
            lease,
            fixture,
        }
    }
    fn account(&self) -> Arc<AtomicUsize> {
        self.source.retained_credit_account()
    }
    fn scratch(&self) -> RetainedByteCredit {
        RetainedByteCredit::reserve(&self.account(), MAX_PENDING_METADATA_BYTES).unwrap()
    }
    fn capture(&self, text: &str) -> NativeNavigationUrl {
        NativeNavigationUrl::capture_bounded(text, text.encode_utf16().count(), &self.account())
            .unwrap()
    }
    fn entry(&self) -> String {
        self.fixture.url("views/editor.html")
    }
    fn facts(
        &self,
        url: NativeNavigationUrl,
        scratch: &RetainedByteCredit,
    ) -> Result<NativeMessageFacts, &'static str> {
        NativeMessageFacts::from_windows_source_url(url, &self.source, &self.lease, scratch)
    }
}

#[test]
fn windows_facts_keep_exact_url_without_native_origin_tuple_or_new_credit() {
    let h = BoundSource::new();
    let account = h.account();
    let baseline = account.load(Ordering::Acquire);
    let scratch = h.scratch();
    let text = format!("{}?appLang=fr&queryObj=%257b", h.entry());
    let url = h.capture(&text);
    let retained = account.load(Ordering::Acquire);
    let facts = h.facts(url, &scratch).unwrap();
    assert_eq!(facts.url(), text);
    assert_eq!(facts.origin(), Some("file://"));
    assert_eq!(facts.native_origin_tuple(), None);
    assert_eq!(
        account.load(Ordering::Acquire),
        retained,
        "qualification reuses the same scratch charge"
    );
    let request = RendererBridgeRequest {
        webview: h.lease.stamp().view(),
        navigation: u64::from(h.lease.stamp().observed_generation()),
        request: 1,
        channel: 0,
        payload: Vec::new(),
        operation: RendererBridgeOperation::Registered,
        document_lease: Some(h.lease.clone()),
        native_facts: Some(facts),
        role_witness: None,
        numeric_result: None,
    };
    assert_eq!(request.url(), Some(text.as_str()));
    assert_eq!(request.origin(), Some("file://"));
    assert_eq!(request.native_origin_tuple(), None);
    drop(scratch);
    assert_eq!(
        account.load(Ordering::Acquire),
        retained - MAX_PENDING_METADATA_BYTES
    );
    drop(request);
    assert_eq!(account.load(Ordering::Acquire), baseline);
}

#[test]
fn current_lease_from_another_actual_source_cannot_qualify_the_url() {
    let a = BoundSource::new();
    let b = BoundSource::new();
    let scratch = a.scratch();
    let account = a.account();
    let before = account.load(Ordering::Acquire);
    assert!(
        NativeMessageFacts::from_windows_source_url(
            a.capture(&a.entry()),
            &a.source,
            &b.lease,
            &scratch,
        )
        .is_err()
    );
    assert_eq!(account.load(Ordering::Acquire), before);
    assert!(a.lease.is_current() && b.lease.is_current());
}

#[test]
fn revoked_source_lease_cannot_produce_new_facts() {
    let mut h = BoundSource::new();
    let scratch = h.scratch();
    let account = h.account();
    let before = account.load(Ordering::Acquire);
    h.owner.destroy();
    assert!(!h.lease.is_current());
    assert!(h.facts(h.capture(&h.entry()), &scratch).is_err());
    assert_eq!(account.load(Ordering::Acquire), before);
}

#[test]
fn informational_file_label_cannot_qualify_another_selected_document() {
    let a = BoundSource::new();
    let b = BoundSource::new();
    let scratch_a = a.scratch();
    let scratch_b = b.scratch();
    let facts_a = a.facts(a.capture(&a.entry()), &scratch_a).unwrap();
    let facts_b = b.facts(b.capture(&b.entry()), &scratch_b).unwrap();
    assert_eq!(facts_a.origin(), facts_b.origin());
    assert_ne!(facts_a.url(), facts_b.url());
    assert!(
        a.source
            .qualify_document_url(facts_a.origin().unwrap())
            .is_err()
    );
    let other = a.fixture.url("views/other.html");
    assert!(
        a.source.qualify_url(&other).is_ok(),
        "same-scheme resource is in the real inventory"
    );
    assert!(
        a.facts(a.capture(&other), &scratch_a).is_err(),
        "origin label grants no document load"
    );
    assert!(
        a.facts(a.capture(facts_b.url()), &scratch_a).is_err(),
        "another root shares no URL authority"
    );
}

#[test]
fn wrong_scratch_account_refuses_without_reserving_another_quota() {
    let h = BoundSource::new();
    let wrong = Arc::new(AtomicUsize::new(0));
    let scratch = RetainedByteCredit::reserve(&wrong, MAX_PENDING_METADATA_BYTES).unwrap();
    let account = h.account();
    let baseline = account.load(Ordering::Acquire);
    assert!(h.facts(h.capture(&h.entry()), &scratch).is_err());
    assert_eq!(account.load(Ordering::Acquire), baseline);
    assert_eq!(wrong.load(Ordering::Acquire), MAX_PENDING_METADATA_BYTES);
    drop(scratch);
    assert_eq!(wrong.load(Ordering::Acquire), 0);
}

#[test]
fn short_existing_scratch_refuses_without_top_up_or_url_retention() {
    let h = BoundSource::new();
    let account = h.account();
    let baseline = account.load(Ordering::Acquire);
    let scratch = RetainedByteCredit::reserve(&account, MAX_PENDING_METADATA_BYTES - 1).unwrap();
    let retained = account.load(Ordering::Acquire);
    assert!(h.facts(h.capture(&h.entry()), &scratch).is_err());
    assert_eq!(account.load(Ordering::Acquire), retained);
    drop(scratch);
    assert_eq!(account.load(Ordering::Acquire), baseline);
}

#[test]
fn correctly_spelled_url_charged_to_another_account_is_not_adoptable() {
    let h = BoundSource::new();
    let scratch = h.scratch();
    let wrong = Arc::new(AtomicUsize::new(0));
    let text = h.entry();
    let url =
        NativeNavigationUrl::capture_bounded(&text, text.encode_utf16().count(), &wrong).unwrap();
    assert!(!url.charged_to(&h.account()));
    let retained = h.account().load(Ordering::Acquire);
    assert!(h.facts(url, &scratch).is_err());
    assert_eq!(h.account().load(Ordering::Acquire), retained);
    assert_eq!(wrong.load(Ordering::Acquire), 0);
}

#[test]
fn credit_observation_requires_actual_account_identity_and_held_extent() {
    let a = Arc::new(AtomicUsize::new(0));
    let b = Arc::new(AtomicUsize::new(0));
    let held_a = RetainedByteCredit::reserve(&a, 64).unwrap();
    let held_b = RetainedByteCredit::reserve(&b, 64).unwrap();
    assert_eq!(a.load(Ordering::Acquire), b.load(Ordering::Acquire));
    assert!(held_a.covers(&a, 64));
    assert!(
        !held_a.covers(&b, 64),
        "equal counter values are not shared custody"
    );
    assert!(!held_a.covers(&a, 65));
    assert!(held_b.covers(&b, 64));
    assert_eq!(
        (a.load(Ordering::Acquire), b.load(Ordering::Acquire)),
        (64, 64)
    );
    drop(held_a);
    drop(held_b);
    assert_eq!(
        (a.load(Ordering::Acquire), b.load(Ordering::Acquire)),
        (0, 0)
    );
}
