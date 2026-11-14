//! Pure ownership witnesses through the actual staging helper; no native calls.
use super::*;
use std::cell::Cell;

struct Witness {
    value: usize,
    drops: Rc<Cell<usize>>,
}
impl Drop for Witness {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn owner() -> (ManagedWindowsOwner<Witness>, Rc<Cell<usize>>) {
    let drops = Rc::new(Cell::new(0));
    (
        ManagedWindowsOwner::new(Witness {
            value: 7,
            drops: Rc::clone(&drops),
        }),
        drops,
    )
}

#[test]
fn native_turn_restores_the_same_map_after_reentrant_unavailability() {
    let (mut owner, drops) = owner();
    let shared = Rc::clone(owner.shared());
    let original = Rc::as_ptr(&shared);
    owner
        .native_turn(|managed| {
            let seen = shared
                .try_borrow_mut()
                .expect("no RefCell borrow crosses entry");
            assert!(seen.is_none(), "reentry cannot obtain the staged map");
            assert_eq!(drops.get(), 0);
            managed.value = 11;
            Ok(())
        })
        .expect("native turn restores");
    assert_eq!(Rc::as_ptr(owner.shared()), original);
    assert!(owner.active.is_none());
    assert_eq!(
        shared.borrow().as_ref().expect("same map restored").value,
        11
    );
    assert_eq!(drops.get(), 0);
    drop(owner);
    assert_eq!(
        drops.get(),
        0,
        "the existing shared owner still holds the map"
    );
    drop(shared);
    assert_eq!(drops.get(), 1);
}

#[test]
fn returned_error_restores_the_same_owner_without_early_drop() {
    let (mut owner, drops) = owner();
    let shared = Rc::clone(owner.shared());
    let error = owner
        .native_turn(|managed| {
            managed.value = 19;
            Err(WvError::EventLoop("owned native turn failed".to_owned()))
        })
        .expect_err("exact closure failure returned");
    assert!(error.to_string().contains("owned native turn failed"));
    assert_eq!(shared.borrow().as_ref().expect("restored on Err").value, 19);
    assert_eq!(drops.get(), 0);
    drop(shared);
    drop(owner);
    assert_eq!(drops.get(), 1);
}

#[test]
fn panic_is_contained_and_original_map_is_restored_before_error() {
    let (mut owner, drops) = owner();
    let shared = Rc::clone(owner.shared());
    assert!(
        owner
            .native_turn(|managed| {
                managed.value = 23;
                assert!(shared.try_borrow().expect("unborrowed cell").is_none());
                panic!("deliberate native-turn panic witness");
            })
            .is_err()
    );
    assert!(owner.active.is_none());
    assert_eq!(
        shared
            .borrow()
            .as_ref()
            .expect("restored after unwind")
            .value,
        23
    );
    assert_eq!(drops.get(), 0);
    drop(shared);
    drop(owner);
    assert_eq!(drops.get(), 1);
}

#[test]
fn busy_before_entry_does_not_move_the_map_or_enter_native_turn() {
    let (mut owner, drops) = owner();
    let shared = Rc::clone(owner.shared());
    let borrowed = shared.borrow();
    let entered = Cell::new(false);
    let result = owner.native_turn(|_| {
        entered.set(true);
        Ok(())
    });
    assert!(result.is_err());
    assert!(!entered.get());
    assert!(owner.active.is_none());
    assert_eq!(borrowed.as_ref().expect("original remains shared").value, 7);
    assert_eq!(drops.get(), 0);
    drop(borrowed);
    drop(shared);
    drop(owner);
    assert_eq!(drops.get(), 1);
}

#[test]
fn held_restore_borrow_retains_active_map_and_refuses_any_second_entry() {
    let (mut owner, drops) = owner();
    let shared = Rc::clone(owner.shared());
    let mut retained_borrow = None;
    let result = owner.native_turn(|managed| {
        managed.value = 29;
        retained_borrow = Some(shared.borrow());
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(
        owner.active.as_ref().expect("same active custody").value,
        29
    );
    assert!(retained_borrow.as_ref().expect("borrow witness").is_none());
    assert_eq!(drops.get(), 0);
    drop(retained_borrow);
    let entered = Cell::new(false);
    assert!(
        owner
            .native_turn(|_| {
                entered.set(true);
                Ok(())
            })
            .is_err()
    );
    assert!(
        !entered.get(),
        "clearing contention is not authority to retry"
    );
    assert!(shared.borrow().is_none());
    assert_eq!(
        owner.active.as_ref().expect("original still staged").value,
        29
    );
    assert_eq!(drops.get(), 0);
    // Only this test's explicit final owner release drops the witness. Production
    // errors retain this entire owner in the existing fatal engine container.
    drop(shared);
    drop(owner);
    assert_eq!(drops.get(), 1);
}

#[test]
fn occupied_restore_slot_preserves_both_values_and_never_replaces_either() {
    let (mut owner, original_drops) = owner();
    let shared = Rc::clone(owner.shared());
    let foreign_drops = Rc::new(Cell::new(0));
    let result = owner.native_turn(|managed| {
        managed.value = 31;
        *shared.borrow_mut() = Some(Witness {
            value: 97,
            drops: Rc::clone(&foreign_drops),
        });
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(owner.active.as_ref().expect("original retained").value, 31);
    assert_eq!(
        shared.borrow().as_ref().expect("collision retained").value,
        97
    );
    assert_eq!(original_drops.get(), 0);
    assert_eq!(foreign_drops.get(), 0);
    assert!(
        owner
            .native_turn(|_| panic!("must not enter a second native turn"))
            .is_err()
    );
    drop(owner);
    assert_eq!(original_drops.get(), 1);
    assert_eq!(foreign_drops.get(), 0);
    drop(shared);
    assert_eq!(foreign_drops.get(), 1);
}
