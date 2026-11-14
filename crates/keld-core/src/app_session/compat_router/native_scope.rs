//! Read-only native backing census on the same two DATA/four CONTROL records.
#[cfg(test)]
use super::{Arc, Direction, NativeRouteKey, PrimaryCallerContext};
use super::{CompatLedger, CompatSession, HostAppError, Ordering, PrimaryRouterHandle, app_detail};
use keld_ipc::window::WindowRef;

impl CompatLedger {
    fn native_storage_released_in(&self, window: Option<WindowRef>) -> bool {
        // Unknown association blocks each physical scope; the unfiltered scan
        // includes every old role/document. This observes only storage release.
        self.controls.iter().flatten().all(|record| {
            record.native_release.as_ref().is_none_or(|released| {
                window
                    .zip(record.native_window)
                    .is_some_and(|(selected, owner)| owner != selected)
                    || released.load(Ordering::Acquire)
            })
        }) && self.data.iter().flatten().all(|record| {
            record.native_storage.as_ref().is_none_or(|released| {
                window
                    .zip(record.native_window)
                    .is_some_and(|(selected, owner)| owner != selected)
                    || released.load(Ordering::Acquire)
            })
        })
    }

    fn endpoint_obligations_quiescent(
        &self,
    ) -> Result<bool, super::super::primary_writer::WriterRefusal> {
        if !self.native_storage_released_in(None)
            || self.preflight.is_some()
            || self.data.iter().any(Option::is_some)
        {
            return Ok(false);
        }
        for record in self.controls.iter().flatten() {
            if !record.retained_obligations_quiescent()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl CompatSession {
    pub(in crate::app_session) fn native_storage_is_quiescent_for(
        &self,
        window: WindowRef,
    ) -> bool {
        self.ledger.native_storage_released_in(Some(window))
    }
    pub(in crate::app_session) fn native_scope_progress(
        &self,
    ) -> Option<super::super::primary_writer::PrimaryWriter> {
        self.ledger.control_progress.clone()
    }
}

impl PrimaryRouterHandle {
    /// Additional endpoint observation after every actual View producer has
    /// closed. W5 still owns numeric Pending and actual dispatcher custody checks.
    /// No Runtime/ring drain or empty receiver substitutes for these records.
    pub(in crate::app_session) fn compat_endpoint_is_quiescent(
        &self,
    ) -> Result<bool, HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(true);
        };
        let state = compat
            .lock()
            .map_err(|_| app_detail("compat endpoint drain", "same ledger lock poisoned"))?;
        state
            .ledger
            .endpoint_obligations_quiescent()
            .map_err(|error| error.into_host_error("compat endpoint physical drain"))
    }
}

#[cfg(test)]
mod tests;
