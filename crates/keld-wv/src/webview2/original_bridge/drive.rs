//! Existing Tao wake/deadline servicing. No new consumer queue or nested pump.
use super::control::{ControlKind, ControlWork};
use super::current::CurrentOperation;
use super::state::State;
use super::{Instant, OriginalBridge, WvError, failure};

struct Driving<'a>(&'a OriginalBridge);
impl Drop for Driving<'_> {
    fn drop(&mut self) {
        self.0.driving.set(false);
    }
}
impl OriginalBridge {
    pub(in crate::webview2) fn drive_on_ui(&self) -> Result<(), WvError> {
        if self.callback_depth.get().is_none() {
            self.fail("bridge callback count is unproved");
            return Err(failure("bridge callback count is unproved"));
        }
        if self.callback_depth.get() != Some(0) || self.capturing.get() || self.installing.get() {
            return Ok(());
        }
        if self.driving.replace(true) {
            self.fail("original bridge service was reentered");
            return Err(failure("original bridge service was reentered"));
        }
        let _driving = Driving(self);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.drive_inner()));
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                self.fail("retained bridge service failed");
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.observe_startup("fault_latched", None, None);
                Err(error)
            }
            Err(_) => {
                self.fail("retained bridge service panicked");
                Err(failure("retained bridge service panicked"))
            }
        }
    }
    fn flush_controls(&self) -> Result<(), WvError> {
        let (wake, cancelled) = {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("control flush native owner borrowed"))?;
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| failure("control flush state borrowed"))?;
            let mut event = self
                .event
                .protocol
                .try_borrow_mut()
                .map_err(|_| failure("control flush event borrowed"))?;
            let State { binding, calls, .. } = &mut *state;
            let progress = calls.flush_controls(
                binding,
                nav.document(),
                &mut event.record,
                &self.endpoint.requests,
            )?;
            let numeric = calls.take_cancelled_numeric_request();
            (progress.needs_service_wake, numeric)
        };
        drop(cancelled); // Token/native input disposal is outside every guard.
        let numeric = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("numeric release owner borrowed"))?
            .calls
            .reconcile_numeric_result()?;
        if wake || numeric {
            self.wake();
        }
        Ok(())
    }
    fn settle_closed_call(&self) -> Result<(), WvError> {
        if !self.close.actual_closed() || self.callback_depth.get() != Some(0) {
            return Ok(());
        }
        let nav = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("closed-call owner borrowed"))?;
        if !nav.callbacks_inactive() || nav.posts_held() {
            return Ok(());
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("closed-call state borrowed"))?;
        if let Some(call) = state.calls.active_mut()
            && call.native_finished()
        {
            let id = call.id();
            let lease = call.lease().clone();
            // Actual same Close plus actual prior API return proves that this
            // original Function cannot remain executing. It is not success.
            call.observe_native_quiescence(id, &lease, false);
        }
        Ok(())
    }
    fn stage_startup_control(&self) -> Result<(), WvError> {
        if self.retired.get() || !self.current.is_empty() {
            return Ok(());
        }
        let selected = {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("startup native owner borrowed"))?;
            let state = self
                .state
                .try_borrow()
                .map_err(|_| failure("startup state borrowed"))?;
            let Some(lease) = nav.current_document() else {
                return Ok(());
            };
            let Some(startup) = state.startup.as_ref() else {
                return Ok(());
            };
            if startup.expired(&state.binding, nav.document(), Instant::now()) {
                drop(state);
                drop(nav);
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.observe_startup("expired", None, None);
                return Err(failure("actual document binding receipt timed out"));
            }
            let kind = if state.registration_ack_pending {
                Some(ControlKind::Registered)
            } else {
                startup
                    .next_bound(&state.binding, nav.document(), Instant::now())
                    .map(ControlKind::Bound)
            };
            let Some(kind) = kind else {
                return Ok(());
            };
            let role = state
                .binding
                .route_role()
                .cloned()
                .or_else(|| nav.host_role().cloned())
                .ok_or_else(|| failure("startup lost actual load caller fence"))?;
            (kind, lease, role, state.epoch()?)
        };
        let (kind, lease, role, epoch) = selected;
        // Core reservation may lock its canonical owner; no native/interior guard
        // survives this call. Full leaves the same startup metadata untouched.
        let mut work = ControlWork::local(kind, lease, role, epoch)?.map(CurrentOperation::Control);
        if work.is_some() && !self.current.attach(&mut work, |v| v) {
            return Err(failure(
                "startup current position changed during pure attachment",
            ));
        }
        Ok(())
    }
    fn drive_inner(&self) -> Result<(), WvError> {
        self.synchronize_commit()?;
        if self.retired.get() {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| failure("retired bridge state borrowed"))?;
            state.calls.cancel_queued()?;
            state.calls.cancel_numeric_result();
            state.binding.discard_revoked_registration();
            state.registration_ack_pending = false;
        }
        self.settle_closed_call()?;
        self.flush_controls()?;
        {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("numeric handoff native owner borrowed"))?;
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| failure("numeric handoff state borrowed"))?;
            state
                .calls
                .flush_numeric_request(nav.document(), &self.endpoint.requests)?;
        }
        self.stage_startup_control()?;
        let disposed = self.current.drive(|operation| operation.drive(self))?;
        if disposed {
            self.wake();
        }
        self.event.drive(self)?;
        self.flush_controls()?;
        self.drive_call()?;
        self.clear_closed_registrations();
        Ok(())
    }
}
