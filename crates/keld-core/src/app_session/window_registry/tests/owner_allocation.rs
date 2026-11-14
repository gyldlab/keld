//! Actual allocation retention in the existing Entry, independent of numeric tags.
use super::*;
use crate::app_session::PrimaryCallerContext;
use std::sync::{Arc, atomic::AtomicBool};

fn caller(attempt: u32) -> PrimaryCallerContext {
    PrimaryCallerContext {
        identity: super::super::super::PrimaryLinkIdentity::Fixture,
        attempt,
        reader_stop: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn create_records_only_the_supplied_actual_caller_allocation() {
    let mut registry = ready();
    let first = caller(1);
    let result = registry
        .create_for_caller(&first, CorrelationId(7), 640, 480, None, None)
        .expect("canonical Create");
    let CreateOutcome::Build { window, .. } = result else {
        panic!("new window");
    };
    let retained = registry
        .entries
        .get(&window.id.0)
        .expect("same Entry")
        .owner_allocation
        .as_ref()
        .expect("actual caller capture");
    assert!(Arc::ptr_eq(retained, &first.reader_stop));
    assert_eq!(
        std::mem::size_of::<Option<Arc<AtomicBool>>>(),
        std::mem::size_of::<usize>()
    );
}

#[test]
fn retained_successor_allocation_survives_its_role_loss_and_absence_is_not_backfilled() {
    let mut registry = ready();
    let a = caller(1);
    let CreateOutcome::Build { build, window, .. } = registry
        .create_for_caller(&a, CorrelationId(7), 640, 480, None, None)
        .expect("A Create")
    else {
        panic!("new window");
    };
    registry.native_built(build, WebviewId(7), snapshot());
    registry.role_lost(owner(1), |_| {});
    let b = caller(1);
    registry.subscribe_successor_for_caller(&b);
    registry.role_lost(owner(1), |_| {});
    let retained = registry
        .entries
        .get(&window.id.0)
        .expect("retained same Entry")
        .owner_allocation
        .as_ref()
        .expect("B survives its retirement");
    assert!(Arc::ptr_eq(retained, &b.reader_stop));
    assert!(!Arc::ptr_eq(retained, &a.reader_stop));
    registry.subscribe_successor(owner(1)); // historical numeric-only fixture
    assert!(
        registry
            .entries
            .get(&window.id.0)
            .expect("same Entry")
            .owner_allocation
            .is_none(),
        "a numeric-only transition never carries A or B's allocation forward"
    );
}
