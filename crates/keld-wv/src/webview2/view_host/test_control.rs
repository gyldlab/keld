//! One-shot, test-only native-operation failure. Never present in shipping builds.
#![forbid(unsafe_code)]
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::Win32::Foundation::E_FAIL;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::webview2) enum Step {
    Hide,
    Bounds,
    Show,
    Focus,
}
struct Pending {
    step: Step,
    fired: Rc<Cell<bool>>,
}
thread_local! {static FAILURE:RefCell<Option<Pending>>=const {RefCell::new(None)};}

pub(in crate::webview2) struct Failure {
    fired: Rc<Cell<bool>>,
}
impl Failure {
    pub(in crate::webview2) fn once(step: Step) -> Self {
        let fired = Rc::new(Cell::new(false));
        FAILURE.with_borrow_mut(|slot| {
            assert!(slot.is_none(), "one native failure per UI thread");
            *slot = Some(Pending {
                step,
                fired: Rc::clone(&fired),
            });
        });
        Self { fired }
    }
    pub(in crate::webview2) fn fired(&self) -> bool {
        self.fired.get()
    }
}
impl Drop for Failure {
    fn drop(&mut self) {
        FAILURE.with_borrow_mut(|slot| {
            // A consumed older guard cannot erase a later independently armed fault.
            slot.take_if(|pending| Rc::ptr_eq(&pending.fired, &self.fired));
        });
    }
}

pub(super) fn before(step: Step) -> windows::core::Result<()> {
    FAILURE.with_borrow_mut(|slot| {
        if let Some(pending) = slot.take_if(|pending| pending.step == step) {
            pending.fired.set(true);
            return Err(E_FAIL.into());
        }
        Ok(())
    })
}
