//! Component discrimination using real retained files plus modeled native facts.
//! These tests do not stand in for actual `WebView2` callback/race qualification.

use super::*;
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, TaskLedger};
use crate::resource_source::windows::tests::Fixture;
use std::sync::atomic::AtomicBool;

struct Harness {
    navigation: OriginalNavigation,
    ledger: TaskLedger,
    fixture: Fixture,
    now: Instant,
}

impl Harness {
    fn new() -> Self {
        let fixture = Fixture::new();
        Self {
            navigation: OriginalNavigation::new(WebviewId(7), fixture.source_arc()).unwrap(),
            ledger: TaskLedger::new(ORIGINAL_WEBAPP_LIMITS),
            fixture,
            now: Instant::now(),
        }
    }

    fn begin(&mut self, query: Option<&str>) -> (NativeAttempt, String, String, ResourceLease) {
        let query = OriginalEntryQuery::from_optional_query(query).unwrap();
        let prepared = self.navigation.prepare(&query).unwrap();
        let uri = prepared.uri().to_owned();
        let header = prepared.header_value().to_owned();
        let actual_input = Rc::clone(&prepared);
        let attempt = self.navigation.begin_load(prepared, self.now).unwrap();
        // Explicitly model the same native invocation entering and returning S_OK.
        assert!(actual_input.begin_native_call());
        assert!(actual_input.native_returned(true));
        let lease = self.navigation.resource_lease().unwrap();
        assert!(self.navigation.current_document().is_none());
        (attempt, uri, header, lease)
    }

    fn start(&mut self, native: u64, uri: &str, header: &str) {
        assert!(self.navigation.navigation_starting(
            id(native),
            uri,
            Some(header),
            false,
            self.now
        ));
    }

    fn serve(&mut self, task: usize, lease: ResourceLease, uri: &str, header: &str) -> TaskTicket {
        let ticket = self.ledger.begin(task).unwrap();
        assert_eq!(
            self.navigation
                .claim_document_request(ticket, lease, uri, Some(header), self.now),
            Ok(true)
        );
        let source = self.fixture.source_arc();
        let selected = source.qualify_url(uri).unwrap();
        let bytes = source
            .verified_read(
                &selected,
                ORIGINAL_WEBAPP_LIMITS.max_asset_bytes,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_ne!(bytes.bytes, [] as [u8; 0]);
        // Models the native adapter's successful SetResponse(200) observation.
        assert!(self.navigation.document_served(ticket, lease, self.now));
        ticket
    }

    fn committed(&mut self, native: u64) -> (String, CheckedNativeDocumentLease) {
        let (_, uri, header, lease) = self.begin(None);
        self.start(native, &uri, &header);
        self.serve(
            usize::try_from(native).expect("modeled native identity fits task key"),
            lease,
            &uri,
            &header,
        );
        self.navigation
            .content_loading(id(native), &uri, false, self.now)
            .unwrap();
        (uri, self.navigation.current_document().unwrap())
    }
}

fn id(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}

#[test]
fn native_id_nonce_selected_response_and_content_commit_are_all_required() {
    let mut h = Harness::new();
    let (_, uri, header, resource) = h.begin(Some("appLang=fr&x=%252f"));
    h.start(41, &uri, &header);
    assert!(h.navigation.current_document().is_none());
    h.serve(101, resource, &uri, &header);
    assert!(h.navigation.current_document().is_none());
    let stamp = h
        .navigation
        .content_loading(id(41), &uri, false, h.now)
        .unwrap();
    assert_eq!(stamp.generation.get(), 2);
    let document = h.navigation.current_document().unwrap();
    assert!(document.belongs_to_source(&h.fixture.source_arc()));
    assert!(document.is_current());
    assert_eq!(
        h.navigation.navigation_completed(id(41), true),
        Some(Ok(stamp))
    );
}

#[test]
fn same_uri_child_without_nonce_cannot_supply_the_top_level_served_witness() {
    let mut h = Harness::new();
    let (_, uri, header, resource) = h.begin(None);
    h.start(42, &uri, &header);
    let child = h.ledger.begin(102).unwrap();
    assert_eq!(
        h.navigation
            .claim_document_request(child, resource, &uri, None, h.now),
        Ok(false)
    );
    assert!(!h.navigation.document_served(child, resource, h.now));
    assert!(
        h.navigation
            .content_loading(id(42), &uri, false, h.now)
            .is_err()
    );
    assert!(h.navigation.current_document().is_none());
}

#[test]
fn same_uri_child_cannot_preempt_or_replace_real_top_level_response() {
    // AC06: an iframe's indistinguishable URI and DOCUMENT resource context
    // must not steal the top-level response before or after it is served.
    // These are state-model controls; native WebView2 attribution remains a
    // separate acceptance requirement.
    for child_first in [true, false] {
        let mut h = Harness::new();
        let (_, uri, header, lease) = h.begin(None);
        h.start(50, &uri, &header);
        if !child_first {
            h.serve(150, lease, &uri, &header);
        }

        let child = h.ledger.begin(151).unwrap();
        assert_eq!(
            h.navigation
                .claim_document_request(child, lease, &uri, None, h.now),
            Ok(false),
            "same-URI child was attributed to the top-level request"
        );
        assert!(
            !h.navigation.document_served(child, lease, h.now),
            "iframe response must not become the top-level served witness"
        );
        if child_first {
            h.serve(152, lease, &uri, &header);
        }

        assert!(h.navigation.current_document().is_none());
        let committed = h
            .navigation
            .content_loading(id(50), &uri, false, h.now)
            .expect("only the real top-level response permits commit");
        assert!(h.navigation.current_document().unwrap().is_current());
        assert_eq!(
            h.navigation.navigation_completed(id(50), true),
            Some(Ok(committed))
        );
    }
}

#[test]
fn same_uri_prior_host_attempt_cannot_satisfy_successor_nonce_or_lease() {
    // AC06: a prior Host attempt may have the exact selected URI, but its
    // nonce and resource lease must not be reusable for the next generation.
    // Model-only test; product WebView2 attribution remains a separate gate.
    for stale_case in 0..3 {
        let mut h = Harness::new();
        let (_, old_uri, old_nonce, old_lease) = h.begin(None);
        h.start(60, &old_uri, &old_nonce);

        let (_, new_uri, new_nonce, new_lease) = h.begin(None);
        assert_eq!(old_uri, new_uri, "same-URI replay is required");
        assert_ne!(old_nonce, new_nonce, "new attempt must have fresh nonce");
        assert_ne!(old_lease.attempt, new_lease.attempt);
        h.start(61, &new_uri, &new_nonce);

        let (lease, nonce) = match stale_case {
            0 => (new_lease, old_nonce.as_str()),
            1 => (old_lease, new_nonce.as_str()),
            _ => (old_lease, old_nonce.as_str()),
        };
        let replay = h.ledger.begin(160).unwrap();
        assert_ne!(
            h.navigation
                .claim_document_request(replay, lease, &new_uri, Some(nonce), h.now),
            Ok(true),
            "prior generation cannot supply a selected-document witness"
        );
        assert!(!h.navigation.document_served(replay, lease, h.now));
        assert!(h.navigation.current_document().is_none());

        if stale_case == 1 {
            // A valid-looking nonce paired with an obsolete lease must fail
            // closed, not allow ContentLoading to commit the new attempt.
            assert!(
                h.navigation
                    .content_loading(id(61), &new_uri, false, h.now)
                    .is_err()
            );
        } else {
            // A wrong nonce is only an ordinary non-selected resource. It
            // cannot suppress the real top-level request of this attempt.
            h.serve(161, new_lease, &new_uri, &new_nonce);
            let committed = h
                .navigation
                .content_loading(id(61), &new_uri, false, h.now)
                .expect("fresh Host request must still qualify");
            assert_eq!(
                h.navigation.navigation_completed(id(61), true),
                Some(Ok(committed))
            );
        }
    }
}

#[test]
fn missing_or_foreign_start_nonce_cannot_bind_native_id() {
    for observed in [None, Some("ffffffffffffffffffffffffffffffff")] {
        let mut h = Harness::new();
        let (_, uri, header, resource) = h.begin(None);
        assert!(
            !h.navigation
                .navigation_starting(id(43), &uri, observed, false, h.now)
        );
        h.navigation.page_navigation_cancelled(id(43));
        let ticket = h.ledger.begin(103).unwrap();
        assert!(
            h.navigation
                .claim_document_request(ticket, resource, &uri, Some(&header), h.now)
                .is_err()
        );
        assert!(h.navigation.current_document().is_none());
    }
}

#[test]
fn explicit_empty_query_is_not_an_absent_query() {
    let mut h = Harness::new();
    let (_, uri, header, _) = h.begin(None);
    assert!(!h.navigation.navigation_starting(
        id(44),
        &format!("{uri}?"),
        Some(&header),
        false,
        h.now
    ));
    assert!(h.navigation.current_document().is_none());

    let (_, uri, header, resource) = h.begin(Some(""));
    assert!(uri.ends_with('?'));
    h.start(45, &uri, &header);
    let ticket = h.ledger.begin(104).unwrap();
    assert!(
        h.navigation
            .claim_document_request(
                ticket,
                resource,
                uri.trim_end_matches('?'),
                Some(&header),
                h.now
            )
            .is_err()
    );
}

#[test]
fn duplicate_selected_document_request_closes_attribution() {
    let mut h = Harness::new();
    let (_, uri, header, resource) = h.begin(None);
    h.start(46, &uri, &header);
    let first = h.ledger.begin(105).unwrap();
    let second = h.ledger.begin(106).unwrap();
    assert_eq!(
        h.navigation
            .claim_document_request(first, resource, &uri, Some(&header), h.now),
        Ok(true)
    );
    assert!(
        h.navigation
            .claim_document_request(second, resource, &uri, Some(&header), h.now)
            .is_err()
    );
    assert!(!h.navigation.document_served(first, resource, h.now));
    assert!(h.navigation.current_document().is_none());
}

#[test]
fn a_verified_response_for_another_native_navigation_id_cannot_commit() {
    let mut h = Harness::new();
    let (_, uri, header, resource) = h.begin(None);
    h.start(47, &uri, &header);
    h.serve(107, resource, &uri, &header);
    assert!(
        h.navigation
            .content_loading(id(48), &uri, false, h.now)
            .is_err()
    );
    assert!(h.navigation.current_document().is_none());
}

#[test]
fn an_error_page_and_expired_attribution_never_mint_a_document() {
    for expired in [false, true] {
        let mut h = Harness::new();
        let (_, uri, header, resource) = h.begin(None);
        h.start(49, &uri, &header);
        h.serve(108, resource, &uri, &header);
        let at = if expired {
            h.now + crate::INITIAL_MILESTONE_DEADLINE
        } else {
            h.now
        };
        assert!(
            h.navigation
                .content_loading(id(49), &uri, !expired, at)
                .is_err()
        );
        assert!(h.navigation.current_document().is_none());
    }
}

#[test]
fn page_cancellation_holds_then_restores_exact_a_without_reminting() {
    let mut h = Harness::new();
    let (uri, document) = h.committed(50);
    let stamp = document.stamp();
    h.navigation.enter_navigation_callback();
    assert!(
        !h.navigation
            .navigation_starting(id(51), &uri, None, false, h.now)
    );
    assert!(document.is_current());
    assert!(!h.navigation.begin_post(&document));
    h.navigation.page_navigation_cancelled(id(51));
    assert!(
        !h.navigation.begin_post(&document),
        "callback hold still owns the cut"
    );
    h.navigation.leave_navigation_callback();
    assert_eq!(h.navigation.current_document().unwrap().stamp(), stamp);
    assert!(h.navigation.begin_post(&document));
    assert!(h.navigation.finish_post());
}

#[test]
fn b_revokes_a_and_late_a_response_or_failure_cannot_borrow_b() {
    let mut h = Harness::new();
    let (_, a_uri, a_header, a_resource) = h.begin(None);
    h.start(52, &a_uri, &a_header);
    let a_ticket = h.serve(109, a_resource, &a_uri, &a_header);
    h.navigation
        .content_loading(id(52), &a_uri, false, h.now)
        .unwrap();
    let a = h.navigation.current_document().unwrap();

    let (_, b_uri, b_header, b_resource) = h.begin(Some("fresh=1"));
    assert!(!a.is_current());
    h.start(53, &b_uri, &b_header);
    h.serve(110, b_resource, &b_uri, &b_header);
    let b_stamp = h
        .navigation
        .content_loading(id(53), &b_uri, false, h.now)
        .unwrap();
    assert!(!h.navigation.document_served(a_ticket, a_resource, h.now));
    assert!(!h.navigation.fail_attempt(a_resource.attempt));
    assert_eq!(h.navigation.navigation_completed(id(52), false), None);
    assert_eq!(h.navigation.current_document().unwrap().stamp().0, b_stamp);
    assert!(!h.navigation.begin_post(&a));
}

#[test]
fn same_digest_in_a_different_retained_source_cannot_begin_load() {
    let mut h = Harness::new();
    let (_, a) = h.committed(54);
    let other = Harness::new();
    let prepared = other
        .navigation
        .prepare(&OriginalEntryQuery::from_optional_query(None).unwrap())
        .unwrap();
    assert!(h.navigation.begin_load(prepared, h.now).is_err());
    assert!(
        a.is_current(),
        "refusal precedes the shared generation transition"
    );
}

#[test]
fn navigation_reentry_inside_post_revokes_and_keeps_outcome_unknown() {
    let mut h = Harness::new();
    let (_, a) = h.committed(55);
    assert!(h.navigation.begin_post(&a));
    h.navigation.enter_navigation_callback();
    assert!(!a.is_current());
    h.navigation.leave_navigation_callback();
    assert!(!h.navigation.finish_post());
    assert!(!h.navigation.begin_post(&a));
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("fresh=1").unwrap())
        .unwrap();
    assert!(
        h.navigation.begin_load(prepared, h.now).is_err(),
        "qualification failure latches"
    );
}

#[test]
fn callback_reentry_before_post_admission_suppresses_the_call() {
    let mut h = Harness::new();
    let (_, a) = h.committed(56);
    h.navigation.enter_navigation_callback();
    assert!(!h.navigation.begin_post(&a));
    h.navigation.leave_navigation_callback();
    assert!(h.navigation.begin_post(&a));
    assert!(h.navigation.finish_post());
}

mod native_call;
