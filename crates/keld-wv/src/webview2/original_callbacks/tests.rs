//! Regression controls for native failure correlation after owner retirement.

use super::*;
use crate::WebviewId;
use crate::engine::OriginalEntryQuery;
use crate::resource_source::windows::tests::Fixture;

#[test]
fn failure_after_destructive_observation_keeps_the_real_started_attempt() {
    let fixture = Fixture::new();
    let mut navigation = OriginalNavigation::new(WebviewId(77), fixture.source_arc()).unwrap();
    let prepared = navigation
        .prepare(&OriginalEntryQuery::new("x=1").unwrap())
        .unwrap();
    let context = NativeNavigationContext::new(NonZeroU64::new(1).unwrap(), WebviewId(77));
    let attempt = navigation
        .begin_host_load(
            prepared,
            context,
            crate::renderer_bridge::RendererRoleWitness::new(std::sync::Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            )),
            Instant::now(),
        )
        .unwrap();
    let state = Rc::new(RefCell::new(navigation));
    let failure = capture_failure(&state).unwrap();
    assert!(state.borrow_mut().fail_attempt(attempt));
    let (send, receive) = std::sync::mpsc::channel();
    let observe: Observation = Rc::new(move |context, fact| send.send((context, fact)).unwrap());
    fail_observed(&state, &observe, failure, &|_| {}).unwrap();
    let (captured, fact) = receive.try_recv().unwrap();
    assert_eq!(captured, Some(context));
    match fact {
        OriginalNavigationFact::Failed {
            attempt: reported,
            revoked: None,
        } => {
            assert_eq!(reported.0, attempt);
        }
        other => panic!("failure lost its actual attempt: {other:?}"),
    }
    assert!(receive.try_recv().is_err());
}

#[test]
fn captured_a_failure_cannot_retire_a_newer_b_attempt() {
    let fixture = Fixture::new();
    let mut navigation = OriginalNavigation::new(WebviewId(78), fixture.source_arc()).unwrap();
    let prepared = navigation
        .prepare(&OriginalEntryQuery::new("a=1").unwrap())
        .unwrap();
    let context_a = NativeNavigationContext::new(NonZeroU64::new(2).unwrap(), WebviewId(78));
    let context_b = NativeNavigationContext::new(NonZeroU64::new(3).unwrap(), WebviewId(78));
    let a = navigation
        .begin_host_load(
            prepared,
            context_a,
            crate::renderer_bridge::RendererRoleWitness::new(std::sync::Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            )),
            Instant::now(),
        )
        .unwrap();
    let state = Rc::new(RefCell::new(navigation));
    let failure_a = capture_failure(&state).unwrap();
    assert!(state.borrow_mut().fail_attempt(a));
    let prepared = state
        .borrow()
        .prepare(&OriginalEntryQuery::new("b=1").unwrap())
        .unwrap();
    let b = state
        .borrow_mut()
        .begin_host_load(
            prepared,
            context_b,
            crate::renderer_bridge::RendererRoleWitness::new(std::sync::Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            )),
            Instant::now(),
        )
        .unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let observe: Observation = Rc::new(move |context, fact| send.send((context, fact)).unwrap());
    fail_observed(&state, &observe, failure_a, &|_| {}).unwrap();
    assert_eq!(state.borrow().active_attempt(), Some(b));
    assert!(state.borrow().resource_lease().is_some());
    let (captured, fact) = receive.try_recv().unwrap();
    assert_eq!(captured, Some(context_a));
    match fact {
        OriginalNavigationFact::Failed {
            attempt: reported, ..
        } => assert_eq!(reported.0, a),
        other => panic!("wrong failure observation: {other:?}"),
    }
}
