//! The same role fence observes/closes its physical Entry without a role grant.
use super::{Arc, CanonicalRendererRoleFence, Weak, with_primary_caller_locked};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod fresh_offer;
impl CanonicalRendererRoleFence {
    pub(super) fn captured_view_scope_status(&self) -> Option<bool> {
        let window = self.window?;
        let current = self.current.upgrade()?;
        let windows = self.windows.as_ref()?.upgrade()?;
        let _compat = self.compat.as_ref()?.upgrade()?;
        let _transition = self.shutdown.transition_guard();
        let _current = current.lock().ok()?;
        windows.lock().ok()?.native_view_scope_status(window)
    }
    pub(super) fn reserve_control_for_captured_window(
        &self,
        encoded: usize,
        accounted: usize,
    ) -> Option<keld_wv::renderer_bridge::NativeControlReservation> {
        let window = self.window?;
        let current = self.current.upgrade()?;
        let windows = self.windows.as_ref()?.upgrade()?;
        let compat = self.compat.as_ref()?.upgrade()?;
        let mut reservation = None;
        with_primary_caller_locked(&current, &self.shutdown, &self.caller, |_| {
            let Ok(windows) = windows.lock() else {
                return Ok(());
            };
            if !windows.native_offers_open_for(&self.caller, window) {
                return Ok(());
            }
            reservation = compat.lock().ok().and_then(|mut state| {
                state.reserve_native_control_for_window(
                    &self.caller,
                    window,
                    encoded,
                    accounted,
                    Arc::clone(&compat),
                )
            });
            Ok(())
        })
        .ok()?;
        reservation
    }
    pub(super) fn close_captured_native_offers(&self) -> bool {
        let Some(window) = self.window else {
            return false;
        };
        let (Some(current), Some(windows), Some(compat)) = (
            self.current.upgrade(),
            self.windows.as_ref().and_then(Weak::upgrade),
            self.compat.as_ref().and_then(Weak::upgrade),
        ) else {
            return false;
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let observed;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let fresh_offer = self.begin_fresh_offer_probe();
        let progress = {
            let _transition = self.shutdown.transition_guard();
            let Ok(_current) = current.lock() else {
                return false;
            };
            let Ok(mut windows) = windows.lock() else {
                return false;
            };
            let Ok(compat) = compat.lock() else {
                return false;
            };
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            let was_open = windows.native_view_scope_status(window) == Some(false);
            if !windows.close_native_view_offers(window) {
                return false;
            }
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            {
                observed = (
                    was_open,
                    windows.native_view_scope_status(window) == Some(true),
                    compat.queued_close_snapshot(window),
                );
            }
            compat.native_scope_progress()
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        {
            let (was_open, offers_closed, snapshot) = observed;
            // Keep the first transition distinct from idempotent drain retries.
            let phase = if was_open {
                "offers-closed"
            } else {
                "offers-still-closed"
            };
            super::super::compat_router::queued_close_observation::observe(
                phase,
                window,
                offers_closed,
                &snapshot,
            );
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(probe) = fresh_offer {
            probe.finish(self);
        }
        if let Some(progress) = progress {
            progress.native_control_progress();
        }
        true
    }
    pub(super) fn captured_native_storage_quiescent(&self) -> bool {
        let Some(window) = self.window else {
            return false;
        };
        let (Some(current), Some(windows), Some(compat)) = (
            self.current.upgrade(),
            self.windows.as_ref().and_then(Weak::upgrade),
            self.compat.as_ref().and_then(Weak::upgrade),
        ) else {
            return false;
        };
        let transition_guard = self.shutdown.transition_guard();
        let Ok(current_guard) = current.lock() else {
            return false;
        };
        let Ok(windows) = windows.lock() else {
            return false;
        };
        if windows.native_view_scope_status(window) != Some(true) {
            return false;
        }
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let offers_closed = windows.native_view_scope_status(window) == Some(true);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let mut observed = None;
        let quiescent = compat.lock().is_ok_and(|state| {
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            {
                observed = Some(state.queued_close_snapshot(window));
            }
            state.native_storage_is_quiescent_for(window)
        });
        // The passive fixed snapshot is printed only after the existing lock
        // order has fully ended. It cannot authorize this result or disposal.
        drop(windows);
        drop(current_guard);
        drop(transition_guard);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if let Some(snapshot) = observed {
            super::super::compat_router::queued_close_observation::observe(
                "storage-polled",
                window,
                offers_closed,
                &snapshot,
            );
        }
        quiescent
    }
}
