//! Pure call/ownership schedules; actual native dispatch is a separate acceptance gate.

use super::*;
use crate::engine::NativeNavigationContext;
use std::sync::atomic::Ordering;

#[test]
fn actual_commit_before_native_return_fails_and_late_return_cannot_repair_it() {
    let mut h = Harness::new();
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("early=1").unwrap())
        .unwrap();
    let uri = prepared.uri().to_owned();
    let header = prepared.header_value().to_owned();
    let actual_input = Rc::clone(&prepared);
    let context = NativeNavigationContext::new(id(1), WebviewId(7));
    let attempt = h
        .navigation
        .begin_host_load(
            prepared,
            context,
            crate::renderer_bridge::RendererRoleWitness::new(std::sync::Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            )),
            h.now,
        )
        .unwrap();
    assert!(actual_input.begin_native_call());
    let lease = h.navigation.resource_lease().unwrap();
    h.start(71, &uri, &header);
    h.serve(171, lease, &uri, &header);
    assert!(!actual_input.native_quiesced());
    assert!(
        h.navigation
            .content_loading(id(71), &uri, false, h.now)
            .is_err()
    );
    assert!(h.navigation.current_document().is_none());
    assert!(actual_input.native_returned(true));
    assert_eq!(h.navigation.active_attempt(), None);
    assert!(!h.navigation.fail_attempt(attempt));
    assert!(
        h.navigation
            .content_loading(id(71), &uri, false, h.now)
            .is_err()
    );
}

#[test]
fn same_preparation_charge_survives_destructive_navigation_until_input_quiesces() {
    let mut h = Harness::new();
    let account = h.fixture.source_arc().retained_credit_account();
    let baseline = account.load(Ordering::Acquire);
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("charge=1").unwrap())
        .unwrap();
    let charged = account.load(Ordering::Acquire);
    assert!(charged > baseline);
    let actual_input = Rc::clone(&prepared);
    let attempt = h.navigation.begin_load(prepared, h.now).unwrap();
    assert!(actual_input.begin_native_call());
    assert!(h.navigation.fail_attempt(attempt));
    assert_eq!(account.load(Ordering::Acquire), charged);
    assert!(!actual_input.native_quiesced());
    assert!(actual_input.native_returned(false));
    assert!(actual_input.native_quiesced());
    assert!(!actual_input.native_succeeded());
    drop(actual_input);
    assert_eq!(account.load(Ordering::Acquire), baseline);
}

#[test]
fn unproved_call_never_reopens_or_becomes_release_credit() {
    let h = Harness::new();
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("unwind=1").unwrap())
        .unwrap();
    assert!(prepared.native_quiesced());
    assert!(prepared.admit_native_call());
    assert!(prepared.begin_native_call());
    assert!(!prepared.native_quiesced());
    prepared.native_unwound();
    assert!(!prepared.native_quiesced());
    assert!(!prepared.native_succeeded());
    assert!(!prepared.admit_native_call());
    assert!(!prepared.begin_native_call());
    assert!(!prepared.native_returned(true));
    assert!(!prepared.native_quiesced());
}

#[test]
fn wrong_view_context_refuses_before_revoking_a_or_entering_b() {
    let mut h = Harness::new();
    let (_, a) = h.committed(72);
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("b=1").unwrap())
        .unwrap();
    let actual_input = Rc::clone(&prepared);
    let wrong = NativeNavigationContext::new(id(2), WebviewId(8));
    assert_eq!(
        h.navigation.begin_host_load(
            prepared,
            wrong,
            crate::renderer_bridge::RendererRoleWitness::new(std::sync::Arc::new(
                std::sync::atomic::AtomicBool::new(false)
            )),
            h.now
        ),
        Err(NativeNavigationRefusal::UnknownView)
    );
    assert!(a.is_current());
    assert!(actual_input.is_prepared());
    assert!(actual_input.native_quiesced());
}

#[test]
fn a_native_return_cannot_advance_a_different_preparation() {
    let h = Harness::new();
    let a = h
        .navigation
        .prepare(&OriginalEntryQuery::new("a=1").unwrap())
        .unwrap();
    let b = h
        .navigation
        .prepare(&OriginalEntryQuery::new("b=1").unwrap())
        .unwrap();
    assert!(a.admit_native_call());
    assert!(a.begin_native_call());
    assert!(b.admit_native_call());
    assert!(b.begin_native_call());
    assert!(a.native_returned(true));
    assert!(a.native_succeeded());
    assert!(!b.native_succeeded());
    assert!(!b.native_quiesced());
}

#[test]
fn inline_reentry_cannot_invoke_the_same_native_call_twice() {
    let h = Harness::new();
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("once=1").unwrap())
        .unwrap();
    assert!(!prepared.begin_native_call());
    assert!(prepared.admit_native_call());
    assert!(!prepared.native_quiesced());
    assert!(prepared.begin_native_call());
    assert!(!prepared.begin_native_call());
    assert!(!prepared.admit_native_call());
    assert!(prepared.native_returned(true));
    assert!(!prepared.begin_native_call());
    assert!(!prepared.admit_native_call());
    assert!(prepared.native_succeeded());
}

#[test]
fn first_challenge_keeps_exact_load_caller_instead_of_view_creator() {
    let mut h = Harness::new();
    let role_a = crate::renderer_bridge::RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let role_b = crate::renderer_bridge::RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let context = NativeNavigationContext::new(id(81), WebviewId(7));
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("role=1").unwrap())
        .unwrap();
    let uri = prepared.uri().to_owned();
    let header = prepared.header_value().to_owned();
    let input = Rc::clone(&prepared);
    h.navigation
        .begin_host_load(prepared, context, role_b.clone(), h.now)
        .unwrap();
    assert!(h.navigation.host_role().unwrap().same_role(&role_b));
    assert!(!h.navigation.host_role().unwrap().same_role(&role_a));
    assert!(input.begin_native_call());
    assert!(input.native_returned(true));
    let resource = h.navigation.resource_lease().unwrap();
    h.start(81, &uri, &header);
    h.serve(181, resource, &uri, &header);
    h.navigation
        .content_loading(id(81), &uri, false, h.now)
        .unwrap();
    assert!(h.navigation.host_role().unwrap().same_role(&role_b));
}

#[test]
fn already_stopped_load_caller_refuses_before_old_document_revocation() {
    let mut h = Harness::new();
    let (_, current) = h.committed(82);
    let role = crate::renderer_bridge::RendererRoleWitness::new(Arc::new(AtomicBool::new(true)));
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("stopped=1").unwrap())
        .unwrap();
    let input = Rc::clone(&prepared);
    let context = NativeNavigationContext::new(id(82), WebviewId(7));
    assert_eq!(
        h.navigation.begin_host_load(prepared, context, role, h.now),
        Err(NativeNavigationRefusal::Unavailable)
    );
    assert!(current.is_current());
    assert!(input.is_prepared());
}

#[test]
fn successful_qualification_records_once_and_never_renews_startup_bound() {
    let mut h = Harness::new();
    let role = crate::renderer_bridge::RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let prepared = h
        .navigation
        .prepare(&OriginalEntryQuery::new("bound=1").unwrap())
        .unwrap();
    let uri = prepared.uri().to_owned();
    let header = prepared.header_value().to_owned();
    let input = Rc::clone(&prepared);
    let context = NativeNavigationContext::new(id(83), WebviewId(7));
    let attempt = h
        .navigation
        .begin_host_load(prepared, context, role, h.now)
        .unwrap();
    assert_eq!(h.navigation.qualified_at(), None);
    assert_eq!(
        h.navigation.next_deadline(),
        h.now.checked_add(crate::INITIAL_MILESTONE_DEADLINE)
    );
    assert!(input.begin_native_call());
    assert!(input.native_returned(true));
    let lease = h.navigation.resource_lease().unwrap();
    h.start(83, &uri, &header);
    h.serve(183, lease, &uri, &header);
    assert_eq!(
        h.navigation.qualified_at(),
        None,
        "serving bytes is not native qualification"
    );
    let qualified = h.now + std::time::Duration::from_secs(1);
    h.navigation
        .content_loading(id(83), &uri, false, qualified)
        .unwrap();
    assert_eq!(h.navigation.qualified_at(), Some(qualified));
    assert_eq!(h.navigation.next_deadline(), None);
    let duplicate = qualified + std::time::Duration::from_secs(3);
    h.navigation
        .content_loading(id(83), &uri, false, duplicate)
        .unwrap();
    assert_eq!(
        h.navigation.qualified_at(),
        Some(qualified),
        "duplicate ContentLoading cannot renew registration"
    );
    h.navigation.fail_attempt(attempt);
    assert_eq!(h.navigation.qualified_at(), None);
    assert_eq!(h.navigation.next_deadline(), None);
}
