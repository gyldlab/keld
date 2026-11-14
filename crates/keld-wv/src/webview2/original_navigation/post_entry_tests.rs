//! Real retained source and modeled native commit; no COM qualification.
use super::*;
use crate::native_document::NativeCommitIdentity;
use crate::resource_source::windows::tests::Fixture;

fn committed() -> (Fixture, OriginalNavigation, CheckedNativeDocumentLease) {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut nav = OriginalNavigation::new(crate::WebviewId(83), source.clone()).unwrap();
    let attempt = nav
        .document_mut()
        .begin_host_load(source.source_id())
        .unwrap();
    nav.document_mut()
        .commit_admitted(
            attempt,
            source.source_id(),
            NativeCommitIdentity(std::num::NonZeroU64::new(27).unwrap()),
        )
        .unwrap();
    let lease = nav.current_document().unwrap();
    (fixture, nav, lease)
}

#[test]
fn refused_local_entry_leaves_same_post_owner_never_entered() {
    let (_fixture, mut nav, lease) = committed();
    let mut calls = 0;
    assert!(!nav.begin_post_with(&lease, &mut |owner| {
        calls += 1;
        assert!(lease.belongs_to_owner(owner));
        false
    }));
    assert_eq!(calls, 1);
    assert!(!nav.posts_held());
    assert!(lease.is_current());
    assert!(
        nav.begin_post(&lease),
        "unchanged wrapper still enters after refusal"
    );
    assert!(nav.finish_post());
}

#[test]
fn accepted_local_entry_observes_exact_owner_and_stages_same_hold_once() {
    let (_fixture, mut nav, lease) = committed();
    let exact = std::ptr::from_ref::<NativeDocumentOwner>(nav.document());
    let mut calls = 0;
    assert!(nav.begin_post_with(&lease, &mut |owner| {
        calls += 1;
        assert_eq!(std::ptr::from_ref(owner), exact);
        assert!(lease.belongs_to_owner(owner));
        true
    }));
    assert!(nav.posts_held());
    assert!(!nav.begin_post_with(&lease, &mut |_| {
        calls += 1;
        true
    }));
    assert_eq!(
        calls, 1,
        "held native entry cannot run another local transition"
    );
    assert!(nav.finish_post());
    nav.destroy();
    assert!(!nav.begin_post_with(&lease, &mut |_| {
        calls += 1;
        true
    }));
    assert_eq!(
        calls, 1,
        "revoked native source cannot call the local entry hook"
    );
}
