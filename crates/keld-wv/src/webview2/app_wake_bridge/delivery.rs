//! The one current command is taken only inside an admitted managed turn.
//! A failed turn acquisition never calls this helper or moves its payload.
#![forbid(unsafe_code)]

pub(in crate::webview2) fn accept<T, E>(
    pending: &mut Option<T>,
    enter: impl FnOnce(T) -> Result<(), E>,
) -> Result<(), E> {
    match pending.take() {
        Some(value) => enter(value),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webview2::managed_owner::ManagedWindowsOwner;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Payload(Rc<Cell<usize>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn refused_native_turn_leaves_the_same_current_payload() {
        let drops = Rc::new(Cell::new(0));
        let mut current = Some(Payload(Rc::clone(&drops)));
        let mut owner = ManagedWindowsOwner::new(None::<Payload>);
        let shared = Rc::clone(owner.shared());
        let held = shared.borrow();
        let result = owner.native_turn(|native| {
            accept(&mut current, |value| {
                *native = Some(value);
                Ok(())
            })
        });
        assert!(result.is_err());
        assert!(current.is_some());
        assert_eq!(drops.get(), 0);
        drop(held);
        drop(current);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn admitted_turn_moves_actual_payload_into_retained_map_before_failure() {
        let drops = Rc::new(Cell::new(0));
        let mut current = Some(Payload(Rc::clone(&drops)));
        let mut owner = ManagedWindowsOwner::new(None::<Payload>);
        let result = owner.native_turn(|native| {
            accept(&mut current, |value| {
                *native = Some(value);
                panic!("after attachment to the actual admitted owner");
            })
        });
        assert!(result.is_err());
        assert!(current.is_none());
        assert_eq!(drops.get(), 0);
        assert!(owner.shared().borrow().as_ref().unwrap().is_some());
        drop(owner);
        assert_eq!(drops.get(), 1);
    }
}
