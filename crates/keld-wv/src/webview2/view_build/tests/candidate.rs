//! Pure Drop controls through the production retain/move/unwind helpers.
//! No native window, interface, Close receipt or runtime success is fabricated.
use super::*;
use std::cell::Cell;
use std::rc::Rc;

struct Witness {
    identity: Rc<()>,
    drops: Rc<Cell<usize>>,
}
impl Drop for Witness {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn witness() -> (Witness, Rc<()>, Rc<Cell<usize>>) {
    let identity = Rc::new(());
    let drops = Rc::new(Cell::new(0));
    (
        Witness {
            identity: Rc::clone(&identity),
            drops: Rc::clone(&drops),
        },
        identity,
        drops,
    )
}
type Resources = BuildResources<Witness, Witness, Witness>;

#[test]
fn inner_unwind_returns_fatal_metadata_while_the_candidate_remains_owned() {
    let (value, identity, drops) = witness();
    let mut candidate: Option<Resources> = None;
    let result: Result<(), BuildFailure> = contain_build(|| {
        candidate = Some(BuildResources::Window(value));
        panic!("after actual candidate staging, before a checked ID exists");
    });
    let Err(failure) = result else {
        panic!("an unwind became build success");
    };
    assert_eq!(failure.disposition, BuildFailureDisposition::Unwound);
    let Some(BuildResources::Window(value)) = candidate.as_ref() else {
        panic!("the original candidate escaped during unwind");
    };
    assert!(Rc::ptr_eq(&value.identity, &identity));
    assert_eq!(drops.get(), 0);
}

#[test]
fn returned_error_and_not_started_refusal_carry_no_native_resources() {
    let (value, identity, drops) = witness();
    let candidate = Some(BuildResources::<Witness, Witness, Witness>::Controller(
        value,
    ));
    let refused = BuildFailure::blocked(super::super::candidate::candidate_error("occupied"));
    assert_eq!(refused.disposition, BuildFailureDisposition::NotStarted);
    let error: Result<(), BuildFailure> = contain_build(|| {
        Err(BuildFailure::webview(
            super::super::candidate::candidate_error("native failure"),
        ))
    });
    assert_eq!(
        error.err().unwrap().disposition,
        BuildFailureDisposition::Returned
    );
    let Some(BuildResources::Controller(value)) = candidate.as_ref() else {
        panic!("changed stage");
    };
    assert!(Rc::ptr_eq(&value.identity, &identity));
    assert_eq!(drops.get(), 0);
}

#[test]
fn a_false_retirement_retains_the_same_candidate_and_returns_no_release_credit() {
    let (value, identity, drops) = witness();
    let mut candidate = Some(value);
    let calls = Cell::new(0);
    assert!(!retire_candidate(&mut candidate, |held| {
        assert!(Rc::ptr_eq(&held.identity, &identity));
        calls.set(calls.get() + 1);
        false
    }));
    assert_eq!(calls.get(), 1);
    assert!(Rc::ptr_eq(&candidate.as_ref().unwrap().identity, &identity));
    assert_eq!(drops.get(), 0);
}

#[test]
fn a_panicking_retirement_keeps_the_candidate_in_place() {
    let (value, identity, drops) = witness();
    let mut candidate = Some(value);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        retire_candidate(&mut candidate, |_| panic!("no retirement return"));
    }));
    assert!(caught.is_err());
    assert!(Rc::ptr_eq(&candidate.as_ref().unwrap().identity, &identity));
    assert_eq!(drops.get(), 0);
}

#[test]
fn only_a_true_retirement_drops_once_and_an_empty_slot_is_not_reentered() {
    let (value, _, drops) = witness();
    let mut candidate = Some(value);
    assert!(retire_candidate(&mut candidate, |_| true));
    assert!(candidate.is_none());
    assert_eq!(drops.get(), 1);
    assert!(retire_candidate(&mut candidate, |_| panic!(
        "no resource remains"
    )));
    assert_eq!(drops.get(), 1);
}

#[test]
fn successful_map_transfer_moves_the_actual_view_without_dropping_it() {
    let (value, identity, drops) = witness();
    let mut candidate: Option<Resources> = Some(BuildResources::View(value));
    let mut views = BTreeMap::new();
    assert!(insert_view_candidate(&mut candidate, &mut views, 8));
    assert!(candidate.is_none());
    assert!(Rc::ptr_eq(&views.get(&8).unwrap().identity, &identity));
    assert_eq!(drops.get(), 0);
    assert!(!insert_view_candidate(&mut candidate, &mut views, 9));
    assert_eq!(views.len(), 1);
    assert_eq!(drops.get(), 0);
}

#[test]
fn map_collision_keeps_both_existing_and_candidate_owners() {
    let (candidate_value, candidate_id, candidate_drops) = witness();
    let (existing, existing_id, existing_drops) = witness();
    let mut candidate: Option<Resources> = Some(BuildResources::View(candidate_value));
    let mut views = BTreeMap::from([(8, existing)]);
    assert!(!insert_view_candidate(&mut candidate, &mut views, 8));
    let Some(BuildResources::View(value)) = candidate.as_ref() else {
        panic!("candidate moved");
    };
    assert!(Rc::ptr_eq(&value.identity, &candidate_id));
    assert!(Rc::ptr_eq(&views.get(&8).unwrap().identity, &existing_id));
    assert_eq!(candidate_drops.get(), 0);
    assert_eq!(existing_drops.get(), 0);
}

#[test]
fn a_wrong_stage_is_restored_before_the_map_helper_returns_refusal() {
    let (value, identity, drops) = witness();
    let mut candidate: Option<Resources> = Some(BuildResources::Window(value));
    let mut views = BTreeMap::new();
    assert!(!insert_view_candidate(&mut candidate, &mut views, 8));
    let Some(BuildResources::Window(value)) = candidate.as_ref() else {
        panic!("stage not restored");
    };
    assert!(Rc::ptr_eq(&value.identity, &identity));
    assert!(views.is_empty());
    assert_eq!(drops.get(), 0);
}

#[test]
fn promotion_stores_the_new_owner_before_old_stage_destructor_observation() {
    struct Disposal<'a> {
        previous: Option<Resources>,
        current: &'a Option<Resources>,
        physical: &'a Rc<()>,
        observed: &'a Cell<bool>,
    }
    impl Drop for Disposal<'_> {
        fn drop(&mut self) {
            // Record the actual current slot at destructor entry; assertions occur
            // after cleanup so a negative control never double-panics in Drop.
            self.observed.set(matches!(
                self.current,
                Some(BuildResources::Controller(value))
                    if Rc::ptr_eq(&value.identity, self.physical)
            ));
            drop(self.previous.take());
        }
    }
    let (old, physical, old_drops) = witness();
    let new_drops = Rc::new(Cell::new(0));
    let new = Witness {
        identity: Rc::clone(&physical),
        drops: Rc::clone(&new_drops),
    };
    let mut candidate: Option<Resources> = Some(BuildResources::Window(old));
    let previous = replace_candidate(&mut candidate, BuildResources::Controller(new));
    assert_eq!(
        old_drops.get(),
        0,
        "the helper must not dispose the previous stage"
    );
    let observed = Cell::new(false);
    drop(Disposal {
        previous,
        current: &candidate,
        physical: &physical,
        observed: &observed,
    });
    assert!(
        observed.get(),
        "old disposal must observe the newly installed owner"
    );
    assert_eq!(old_drops.get(), 1);
    assert_eq!(new_drops.get(), 0, "same physical allocation is still held");
}
