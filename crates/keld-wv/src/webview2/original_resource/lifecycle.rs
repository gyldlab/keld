//! Retirement preserves native/body custody until actual completion or Close.

use super::{NativePending, ResourceSession, ResourceView, State, native};
use crate::{WebviewId, WvError};
use std::sync::atomic::Ordering;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2WebResourceRequestedEventArgs;

impl ResourceSession {
    pub(super) fn mark_view_stopped(&self, view: WebviewId) {
        let mut state = self.state.borrow_mut();
        let State {
            ledger,
            pending,
            admitting,
            ..
        } = &mut *state;
        for (key, pending) in pending
            .iter()
            .filter(|(_, pending)| pending.view_id == view)
        {
            pending.cancelled.store(true, Ordering::Release);
            ledger.stop(*key);
        }
        for (key, _) in admitting.iter().filter(|(_, owner)| **owner == view) {
            ledger.stop(*key);
        }
    }

    fn cancel_view(&self, view: &ResourceView) {
        use std::ops::Bound::{Excluded, Unbounded};
        self.mark_view_stopped(view.id);
        let mut after = None;
        loop {
            let key = self
                .state
                .borrow()
                .pending
                .range((after.map_or(Unbounded, Excluded), Unbounded))
                .find(|(_, pending)| pending.view_id == view.id)
                .map(|(key, _)| *key);
            let Some(key) = key else {
                break;
            };
            after = Some(key);
            self.cancel_ticket(view, key);
        }
    }

    pub(super) fn cancel_ticket(&self, view: &ResourceView, key: usize) {
        let pending = {
            let mut state = self.state.borrow_mut();
            state.ledger.stop(key);
            let pending = state.pending.remove(&key);
            if pending.is_some() {
                state.admitting.insert(key, view.id);
            }
            pending
        };
        let Some(mut pending) = pending else {
            return;
        };
        pending.cancelled.store(true, Ordering::Release);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.refuse_pending(&mut pending)
        }));
        let settled = if let Ok(settled) = result {
            settled
        } else {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                view.emergency_close("resource cancellation callback panicked");
            }));
            view.controller_closed.get()
        };
        let worker_quiescent = pending.body_credit.is_some() || pending.completed.is_some();
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.observe_pending("cancel-settled", &pending);
        if settled && worker_quiescent {
            let delivered = pending.completed.is_some();
            self.release_pending(pending, delivered);
        } else {
            self.state.borrow_mut().pending.insert(key, pending);
        }
        self.state.borrow_mut().admitting.remove(&key);
    }

    /// One Host-wide worker is retired only at Host shutdown, never per window.
    pub(in crate::webview2) fn retire_on_ui(&self) -> Result<(), WvError> {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let first_retirement = !self.retiring.get();
        self.retiring.set(true);
        self.state.borrow_mut().ledger.stop_all();
        let mut after = None;
        while let Some((id, view)) = self.next_view_after(after) {
            after = Some(id);
            let Some(view) = view else {
                self.latch_failure("resource view owner disappeared before actual Close", id);
                continue;
            };
            view.retired.set(true);
            view.revoke_navigation();
            self.cancel_view(&view);
        }
        self.reader.borrow_mut().retire()?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if first_retirement {
            self.observe_resources("session-retiring", None);
        }
        self.drain_ui()
    }
}

impl ResourceView {
    pub(super) fn revoke_navigation(&self) {
        if let Ok(mut navigation) = self.navigation.try_borrow_mut() {
            navigation.destroy();
        } else {
            self.session.latch_failure(
                "resource revocation met a held document-owner borrow",
                self.id,
            );
        }
    }

    /// On Err, the caller MUST retain this owner and must not bypass it with
    /// an unconditional Close: it may be preserving the environment's last guard.
    pub(in crate::webview2) fn retire_on_ui(&self) -> Result<(), WvError> {
        self.retired.set(true);
        self.revoke_navigation();
        self.session.cancel_view(self);
        if self.controller_closed.get() {
            return Ok(());
        }
        if let Err(error) = self.session.transfer_worker_filter(self) {
            self.close_after_failed_transfer(
                "worker filter custody could not transfer before Close",
            );
            return Err(error);
        }
        self.session.reserve_guard_close(self)?;
        Ok(())
    }

    /// The caller has just observed this exact retained controller's successful
    /// Close. This is not a worker-exit receipt and never releases active I/O.
    pub(in crate::webview2) fn controller_closed_on_ui(&self) {
        if self.controller_closed.replace(true) {
            return;
        }
        self.retired.set(true);
        self.revoke_navigation();
        self.session.cancel_view(self);
        let emergency = self.emergency.borrow_mut().take();
        drop(emergency);
        self.release_closed_registration();
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.session
            .observe_resources("controller-closed", Some(self.id));
    }

    pub(super) fn release_closed_registration(&self) {
        if !self.controller_closed.get()
            || self.emergency_acquiring.get()
            || self.install_in_progress.get()
            || self.emergency.borrow().is_some()
        {
            return;
        }
        let mut state = self.session.state.borrow_mut();
        if !state
            .views
            .get(&self.id.0)
            .is_some_and(|view| std::ptr::eq(view.as_ptr(), self))
        {
            return;
        }
        if state.worker_filter_owner.get(&self.environment_identity) == Some(&self.id) {
            state.worker_filter_owner.remove(&self.environment_identity);
        }
        if state.closing_filter_owner.get(&self.environment_identity) == Some(&self.id) {
            state
                .closing_filter_owner
                .remove(&self.environment_identity);
        }
        state.views.remove(&self.id.0);
    }

    pub(super) fn emergency_close(&self, detail: &'static str) {
        self.session.latch_failure(detail, self.id);
        self.retired.set(true);
        self.revoke_navigation();
        self.session.mark_view_stopped(self.id);
        // No source owner/RefCell borrow survives entry to the same W2 Close.
        if self.session.transfer_worker_filter(self).is_err() {
            self.close_after_failed_transfer(detail);
            return;
        }
        if self.session.reserve_guard_close(self).is_ok() && self.close.close_on_ui() {
            self.controller_closed_on_ui();
        }
    }

    fn close_after_failed_transfer(&self, detail: &'static str) {
        self.session.latch_failure(detail, self.id);
        // A proved successor may already own coverage when removal of the old
        // filter failed. Otherwise close every affected view while retaining this
        // old guard. An unproved receipt deliberately keeps it alive.
        let safe = self.session.has_other_guard(self) || self.session.contain_other_views(self);
        if safe && self.session.reserve_guard_close(self).is_ok() && self.close.close_on_ui() {
            self.controller_closed_on_ui();
        }
    }

    pub(super) fn hold_then_emergency_close(
        &self,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
        detail: &'static str,
    ) {
        // Reserve occupancy before GetDeferral, which can reenter this callback.
        // A nested attempt may close the controller, but cannot acquire or replace
        // another emergency deferral while this exact acquisition is in flight.
        if !self.emergency_acquiring.replace(true) {
            let empty = self.emergency.borrow().is_none();
            if empty && let Ok(deferral) = native::defer(args) {
                if self.controller_closed.get() {
                    drop(deferral);
                } else {
                    let native = NativePending {
                        args: args.clone(),
                        deferral: Some(deferral),
                        environment: self.environment.clone(),
                        response: None,
                    };
                    // AddRef can reenter too. The actual Close receipt permits
                    // disposal; otherwise this reserved slot has no other writer.
                    if self.controller_closed.get() {
                        drop(native);
                    } else {
                        *self.emergency.borrow_mut() = Some(native);
                    }
                }
            }
            self.emergency_acquiring.set(false);
            self.release_closed_registration();
        }
        self.emergency_close(detail);
    }
}
