//! Real source/lease ownership with a modeled commit; no native Close is fabricated.
use super::*;
use crate::WebviewId;
use crate::native_document::NativeCommitIdentity;
use crate::resource_source::windows::tests::Fixture;
use std::num::NonZeroU64;
use std::sync::Arc;

#[test]
fn busy_retirement_preserves_retry_then_revokes_the_same_live_document_once() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let mut navigation = OriginalNavigation::new(WebviewId(7), Arc::clone(&source)).unwrap();
    let attempt = navigation
        .document_mut()
        .begin_host_load(source.source_id())
        .unwrap();
    let stamp = navigation
        .document_mut()
        .commit_admitted(
            attempt,
            source.source_id(),
            NativeCommitIdentity(NonZeroU64::new(97).unwrap()),
        )
        .unwrap();
    let document = navigation.current_document().unwrap();
    let navigation = RefCell::new(navigation);
    let retired = Cell::new(false);
    let callback_hold = navigation.borrow();
    assert!(retire_navigation(&navigation, &retired).is_err());
    assert!(!retired.get(), "failed borrow cannot commit retirement");
    assert!(document.is_current(), "the first attempt did not revoke");
    drop(callback_hold);
    let observed = retire_navigation(&navigation, &retired).unwrap();
    assert_eq!(observed, Some((None, Some(NativeDocumentStamp(stamp)))));
    assert!(retired.get());
    assert!(
        !document.is_current(),
        "retry must revoke the SAME live lease"
    );
    assert_eq!(retire_navigation(&navigation, &retired).unwrap(), None);
}
