//! State controls with real retained source/lease owners, not native execution.
use super::*;
use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
use crate::renderer_contract::{CheckedNativeDocumentLease, NativeAttemptObservation};
use crate::resource_source::windows::tests::Fixture;
use std::sync::Arc;

const APP: &str = "0123456789abcdef0123456789abcdef";
const TRUSTED: &str = "abcdef0123456789abcdef0123456789";

struct Bound {
    owner: NativeDocumentOwner,
    binding: OriginalBinding,
    lease: CheckedNativeDocumentLease,
    source: Arc<crate::resource_source::AdmittedWebAppSource>,
    _fixture: Fixture,
}
impl Bound {
    fn new() -> Self {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let (owner, binding, lease) = Self::bind(&source);
        Self {
            owner,
            binding,
            lease,
            source,
            _fixture: fixture,
        }
    }
    fn bind(
        source: &Arc<crate::resource_source::AdmittedWebAppSource>,
    ) -> (
        NativeDocumentOwner,
        OriginalBinding,
        CheckedNativeDocumentLease,
    ) {
        let mut owner = NativeDocumentOwner::new(WebviewId(9));
        owner.bind_source(source).unwrap();
        let attempt = owner.begin_host_load(source.source_id()).unwrap();
        let stamp = owner
            .commit_admitted(
                attempt,
                source.source_id(),
                NativeCommitIdentity(NonZeroU64::new(41).unwrap()),
            )
            .unwrap();
        let lease = CheckedNativeDocumentLease(owner.lease_for(stamp).unwrap());
        let mut binding = OriginalBinding::default();
        binding
            .prepare_attempt(
                &owner,
                NativeAttemptObservation(attempt),
                APP.into(),
                TRUSTED.into(),
            )
            .unwrap();
        let commit = binding.take_matching_commit(&owner).unwrap().unwrap();
        binding.finish_commit(&owner, commit).unwrap();
        (owner, binding, lease)
    }
}

#[test]
fn absolute_bound_starts_at_qualification_not_later_drive() {
    let h = Bound::new();
    let qualified = Instant::now();
    let state = StartupReceipt::new(&h.owner, h.lease.clone(), qualified).unwrap();
    let deadline = qualified + Duration::from_secs(5);
    assert_eq!(state.next_deadline(&h.binding, &h.owner), Some(deadline));
    assert_eq!(
        state.next_bound(&h.binding, &h.owner, qualified + Duration::from_secs(4)),
        Some(BoundEntry::Initial)
    );
    assert!(!state.expired(
        &h.binding,
        &h.owner,
        deadline.checked_sub(Duration::from_nanos(1)).unwrap()
    ));
    assert!(state.expired(&h.binding, &h.owner, deadline));
    assert_eq!(state.next_bound(&h.binding, &h.owner, deadline), None);
}

#[test]
fn only_one_initial_and_one_actual_dom_retry_may_enter() {
    let h = Bound::new();
    let now = Instant::now();
    let mut state = StartupReceipt::new(&h.owner, h.lease.clone(), now).unwrap();
    assert!(!state.begin_bound(BoundEntry::DomRetry, &h.binding, &h.owner, now));
    assert!(state.begin_bound(BoundEntry::Initial, &h.binding, &h.owner, now));
    assert!(!state.begin_bound(BoundEntry::Initial, &h.binding, &h.owner, now));
    assert_eq!(state.next_bound(&h.binding, &h.owner, now), None);
    assert!(state.observe_dom_content_loaded(&h.owner, NonZeroU64::new(41).unwrap()));
    assert!(state.begin_bound(BoundEntry::DomRetry, &h.binding, &h.owner, now));
    assert!(!state.begin_bound(BoundEntry::DomRetry, &h.binding, &h.owner, now));
    assert_eq!(state.next_bound(&h.binding, &h.owner, now), None);
    assert!(
        !h.binding.registered(),
        "entry is not a registration receipt"
    );
    assert_eq!(
        state.next_deadline(&h.binding, &h.owner),
        Some(now + RECEIPT_BOUND)
    );
}

#[test]
fn duplicate_or_foreign_dom_cannot_create_retry_or_renew_deadline() {
    let h = Bound::new();
    let now = Instant::now();
    let mut state = StartupReceipt::new(&h.owner, h.lease.clone(), now).unwrap();
    assert!(state.begin_bound(BoundEntry::Initial, &h.binding, &h.owner, now));
    assert!(!state.observe_dom_content_loaded(&h.owner, NonZeroU64::new(42).unwrap()));
    assert_eq!(state.next_bound(&h.binding, &h.owner, now), None);
    assert!(state.observe_dom_content_loaded(&h.owner, NonZeroU64::new(41).unwrap()));
    assert!(!state.observe_dom_content_loaded(&h.owner, NonZeroU64::new(41).unwrap()));
    assert_eq!(
        state.next_deadline(&h.binding, &h.owner),
        Some(now + RECEIPT_BOUND)
    );
}

#[test]
fn only_exact_current_binding_registration_settles_startup() {
    let mut h = Bound::new();
    let now = Instant::now();
    let state = StartupReceipt::new(&h.owner, h.lease.clone(), now).unwrap();
    let (other_owner, mut other_binding, other_lease) = Bound::bind(&h.source);
    other_binding.register(&other_owner, APP).unwrap();
    assert_eq!(
        other_lease.0.stamp(),
        h.lease.0.stamp(),
        "numeric observations intentionally coincide"
    );
    assert!(
        !state.matches_document(&other_lease),
        "owner allocation still differs"
    );
    assert_eq!(
        state.next_deadline(&other_binding, &h.owner),
        Some(now + RECEIPT_BOUND)
    );
    assert!(h.binding.register(&h.owner, "wrong").is_err());
    assert_eq!(
        state.next_deadline(&h.binding, &h.owner),
        Some(now + RECEIPT_BOUND)
    );
    h.binding.register(&h.owner, APP).unwrap();
    assert_eq!(state.next_deadline(&h.binding, &h.owner), None);
    assert_eq!(state.next_bound(&h.binding, &h.owner, now), None);
    assert!(!state.expired(&h.binding, &h.owner, now + Duration::from_secs(9)));
}

#[test]
fn revoked_or_same_source_foreign_owner_cannot_mint_an_entry() {
    let mut h = Bound::new();
    let now = Instant::now();
    let (other, _, _) = Bound::bind(&h.source);
    assert!(StartupReceipt::new(&other, h.lease.clone(), now).is_err());
    let mut state = StartupReceipt::new(&h.owner, h.lease.clone(), now).unwrap();
    h.owner.destroy();
    assert_eq!(state.next_deadline(&h.binding, &h.owner), None);
    assert_eq!(state.next_bound(&h.binding, &h.owner, now), None);
    assert!(!state.observe_dom_content_loaded(&h.owner, NonZeroU64::new(41).unwrap()));
    assert!(!state.begin_bound(BoundEntry::Initial, &h.binding, &h.owner, now));
    // This metadata owner has no native/quota-release API.
}
