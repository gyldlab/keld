//! Actual worker filter receipts and exact-environment close containment.

use super::{ResourceSession, ResourceView, failure, native};
use crate::WvError;
use std::rc::Rc;

impl ResourceSession {
    pub(super) fn ensure_worker_filter(&self, view: &Rc<ResourceView>) -> Result<(), WvError> {
        {
            let mut state = self.state.borrow_mut();
            if state
                .closing_filter_owner
                .contains_key(&view.environment_identity)
            {
                return Err(failure("the environment's last file guard is closing"));
            }
            if let Some(owner) = state.worker_filter_owner.get(&view.environment_identity) {
                let live = state
                    .views
                    .get(&owner.0)
                    .and_then(std::rc::Weak::upgrade)
                    .is_some_and(|view| !view.controller_closed.get());
                return if live {
                    Ok(())
                } else {
                    Err(failure("worker filter owner is no longer retained"))
                };
            }
            if !state.filter_transition.insert(view.environment_identity) {
                return Err(failure("worker filter installation reentered"));
            }
        }
        let installed = native::add_filter(&view.core, true);
        self.state
            .borrow_mut()
            .filter_transition
            .remove(&view.environment_identity);
        if installed.is_err() {
            return Err(failure(
                "cannot install the environment's worker file filter",
            ));
        }
        if view.retired.get()
            || view.controller_closed.get()
            || self.retiring.get()
            || self.fault.get().is_some()
        {
            // S_OK does not override a retirement observed during the native call.
            let _ = native::remove_worker_filter(&view.core);
            return Err(failure("worker filter view retired during installation"));
        }
        self.state
            .borrow_mut()
            .worker_filter_owner
            .insert(view.environment_identity, view.id);
        Ok(())
    }

    pub(super) fn transfer_worker_filter(&self, old: &ResourceView) -> Result<(), WvError> {
        let successor = {
            let state = self.state.borrow();
            if state.filter_transition.contains(&old.environment_identity) {
                return Err(failure(
                    "worker filter retirement reentered a native transition",
                ));
            }
            if state.worker_filter_owner.get(&old.environment_identity) != Some(&old.id) {
                drop(state);
                return if self.has_other_guard(old) || self.contain_other_views(old) {
                    Ok(())
                } else {
                    Err(failure(
                        "worker filter coverage is unproved during retirement",
                    ))
                };
            }
            state
                .views
                .values()
                .filter_map(std::rc::Weak::upgrade)
                .find(|view| {
                    view.id != old.id
                        && view.environment_identity == old.environment_identity
                        && !view.retired.get()
                        && !view.controller_closed.get()
                })
        };
        // The last view keeps its refusal callback/filter through actual Close.
        // A later view installs its own filter before any navigation is issued.
        let Some(successor) = successor else {
            return if self.contain_other_views(old) {
                Ok(())
            } else {
                Err(failure("retiring environment still has an unclosed view"))
            };
        };
        {
            let mut state = self.state.borrow_mut();
            if !state.filter_transition.insert(old.environment_identity) {
                return Err(failure("worker filter transfer reentered"));
            }
        }
        let added = native::add_filter(&successor.core, true);
        if added.is_err() {
            self.state
                .borrow_mut()
                .filter_transition
                .remove(&old.environment_identity);
            return Err(failure(
                "successor worker file filter could not be installed",
            ));
        }
        if successor.retired.get() || successor.controller_closed.get() {
            let _ = native::remove_worker_filter(&successor.core);
            self.state
                .borrow_mut()
                .filter_transition
                .remove(&old.environment_identity);
            return Err(failure(
                "worker filter successor retired during installation",
            ));
        }
        self.state
            .borrow_mut()
            .worker_filter_owner
            .insert(old.environment_identity, successor.id);
        let removed = native::remove_worker_filter(&old.core);
        self.state
            .borrow_mut()
            .filter_transition
            .remove(&old.environment_identity);
        if removed.is_err() {
            return Err(failure("old worker file filter removal is unproved"));
        }
        if successor.retired.get()
            || successor.controller_closed.get()
            || self
                .state
                .borrow()
                .worker_filter_owner
                .get(&old.environment_identity)
                != Some(&successor.id)
        {
            return Err(failure("worker filter successor changed during transfer"));
        }
        Ok(())
    }

    pub(super) fn has_other_guard(&self, old: &ResourceView) -> bool {
        let state = self.state.borrow();
        if state.filter_transition.contains(&old.environment_identity)
            || state
                .closing_filter_owner
                .contains_key(&old.environment_identity)
        {
            return false;
        }
        state
            .worker_filter_owner
            .get(&old.environment_identity)
            .filter(|owner| **owner != old.id)
            .and_then(|owner| state.views.get(&owner.0))
            .and_then(std::rc::Weak::upgrade)
            .is_some_and(|view| !view.controller_closed.get())
    }

    pub(super) fn contain_other_views(&self, old: &ResourceView) -> bool {
        // An in-flight native transfer has no receipt yet. Preserve its guard.
        if self
            .state
            .borrow()
            .filter_transition
            .contains(&old.environment_identity)
        {
            return false;
        }
        let mut after = None;
        while let Some((id, view)) = self.next_view_after(after) {
            after = Some(id);
            if let Some(view) = view.filter(|view| {
                view.id != old.id
                    && view.environment_identity == old.environment_identity
                    && !view.controller_closed.get()
            }) {
                view.retired.set(true);
                view.revoke_navigation();
                self.mark_view_stopped(view.id);
            }
        }
        // All affected sources are revoked before any native Close. Cursor
        // iteration retains only the one actual view currently being closed.
        after = None;
        while let Some((id, view)) = self.next_view_after(after) {
            after = Some(id);
            if let Some(view) = view.filter(|view| {
                view.id != old.id
                    && view.environment_identity == old.environment_identity
                    && !view.controller_closed.get()
            }) && view.close.close_on_ui()
            {
                view.controller_closed_on_ui();
            }
        }
        let closed = self.state.borrow().views.values().all(|view| {
            view.upgrade().is_some_and(|view| {
                view.id == old.id
                    || view.environment_identity != old.environment_identity
                    || view.controller_closed.get()
            })
        });
        if !closed {
            self.latch_failure(
                "environment containment has an unproved controller Close",
                old.id,
            );
        }
        closed
    }

    pub(super) fn reserve_guard_close(&self, old: &ResourceView) -> Result<(), WvError> {
        if old.controller_closed.get() || self.has_other_guard(old) {
            return Ok(());
        }
        let mut state = self.state.borrow_mut();
        if state.filter_transition.contains(&old.environment_identity)
            || state.views.values().any(|view| {
                view.upgrade().is_none_or(|view| {
                    view.id != old.id
                        && view.environment_identity == old.environment_identity
                        && !view.controller_closed.get()
                })
            })
        {
            return Err(failure(
                "cannot close the environment's remaining file guard",
            ));
        }
        // Block reentrant new-view admission until this exact actual Close is
        // observed. An old map entry alone cannot justify a new view's coverage.
        state
            .closing_filter_owner
            .insert(old.environment_identity, old.id);
        Ok(())
    }
}
