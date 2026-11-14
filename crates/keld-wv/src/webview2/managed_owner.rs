//! Stage the same managed map outside `RefCell` while native entry may reenter.
//! A failed restoration leaves the actual map in this owner for fatal retention.
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::managed_app::ManagedWindows;
use crate::WvError;

pub(super) type ManagedWindowsShared<T = ManagedWindows> = Rc<RefCell<Option<T>>>;

pub(super) struct ManagedWindowsOwner<T = ManagedWindows> {
    shared: ManagedWindowsShared<T>,
    active: Option<T>,
}

impl<T> ManagedWindowsOwner<T> {
    pub(super) fn new(managed: T) -> Self {
        Self {
            shared: Rc::new(RefCell::new(Some(managed))),
            active: None,
        }
    }

    pub(super) fn shared(&self) -> &ManagedWindowsShared<T> {
        &self.shared
    }

    /// No `RefCell` borrow crosses `turn`. The closure returns only unit or the
    /// existing error type; no owned native result can be dropped on restore failure.
    /// After any error the caller must retain this entire owner and end its loop.
    pub(super) fn native_turn(
        &mut self,
        turn: impl FnOnce(&mut T) -> Result<(), WvError>,
    ) -> Result<(), WvError> {
        if self.active.is_some() {
            return Err(owner_error(
                "a previous native turn remains staged; retain the owner and stop the loop",
            ));
        }
        {
            let mut shared = self.shared.try_borrow_mut().map_err(|_| {
                owner_error("the managed map is borrowed before native entry; retain the owner and stop the loop")
            })?;
            // Move directly into the same owner's field. No callback, allocation,
            // fallible operation or native entry can intervene in this transfer.
            self.active = shared.take();
        }
        let outcome = match self.active.as_mut() {
            Some(managed) => catch_unwind(AssertUnwindSafe(|| turn(managed))),
            None => {
                return Err(owner_error(
                    "the managed map is unavailable before native entry; retain the owner and stop the loop",
                ));
            }
        };
        {
            let mut shared = self.shared.try_borrow_mut().map_err(|_| {
                owner_error("the managed map is borrowed during restoration; the exact active map remains retained")
            })?;
            if shared.is_some() {
                // A collision is an invariant failure. Never replace/drop the
                // conflicting value, and keep the original active map here.
                return Err(owner_error(
                    "the managed map restoration slot is occupied; retain both values and stop the loop",
                ));
            }
            *shared = self.active.take();
        }
        match outcome {
            Ok(result) => result,
            Err(_) => Err(owner_error(
                "native managed turn panicked; the exact map was restored for retained shutdown",
            )),
        }
    }
}

fn owner_error(detail: &str) -> WvError {
    WvError::EventLoop(detail.to_owned())
}

#[cfg(test)]
#[path = "managed_owner/tests.rs"]
mod tests;
