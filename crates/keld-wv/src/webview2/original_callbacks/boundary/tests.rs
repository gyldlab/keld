//! Pure same-owner callback schedules; no native COM delivery is claimed.

use super::*;
use crate::resource_source::windows::tests::Fixture;
use std::cell::Cell;

fn fixture() -> (Fixture, Rc<RefCell<OriginalNavigation>>) {
    let source = Fixture::new();
    let state = OriginalNavigation::new(WebviewId(91), source.source_arc()).unwrap();
    (source, Rc::new(RefCell::new(state)))
}

#[test]
fn observer_panic_is_contained_while_the_same_callback_hold_is_live() {
    let (_source, state) = fixture();
    let faults = Cell::new(0);
    let failed = |_| faults.set(faults.get() + 1);
    let observer_state = Rc::clone(&state);
    let observe: Observation = Rc::new(move |_, _| {
        assert!(!observer_state.borrow().callbacks_inactive());
        panic!("observer failed during held callback");
    });
    let failure = capture_failure(&state).unwrap();
    let result = boundary(&failed, || {
        callback(&state, &failed, || {
            let result = fail_observed(&state, &observe, failure, &failed);
            assert!(!state.borrow().callbacks_inactive());
            result
        })
    });
    assert!(result.is_err());
    assert_eq!(faults.get(), 1);
    assert!(state.borrow().callbacks_inactive());
}

#[test]
fn rejected_entry_latches_existing_failure_sink_before_returning_without_a_hold() {
    let (_source, state) = fixture();
    let held = state.borrow_mut();
    let fault = Cell::new(false);
    let ran = Cell::new(false);
    let result = boundary(&|_| fault.set(true), || {
        callback(&state, &|_| fault.set(true), || {
            ran.set(true);
            Ok(())
        })
    });
    assert!(result.is_err());
    assert!(
        fault.get(),
        "zero depth alone must not grant release after refused entry"
    );
    assert!(!ran.get());
    drop(held);
    assert!(state.borrow().callbacks_inactive());
}

#[test]
fn rejected_exit_keeps_original_depth_and_latches_failure_instead_of_quiescence() {
    let (_source, state) = fixture();
    let fault = Cell::new(false);
    let mut held = None;
    let result = boundary(&|_| fault.set(true), || {
        callback(&state, &|_| fault.set(true), || {
            held = Some(state.borrow_mut());
            Ok(())
        })
    });
    assert!(result.is_ok()); // COM return is not the existing session's release result.
    assert!(fault.get());
    drop(held);
    assert!(!state.borrow().callbacks_inactive());
    state.borrow_mut().leave_navigation_callback(); // Test-only cleanup of deliberately violated borrowing.
}

#[test]
fn outer_boundary_also_contains_a_preamble_panic_before_hold_entry() {
    let fault = Cell::new(false);
    assert!(boundary(&|_| fault.set(true), || panic!("entry preamble failed")).is_err());
    assert!(fault.get());
}
