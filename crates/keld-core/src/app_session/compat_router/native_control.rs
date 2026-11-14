//! Shared GH540 CONTROL accounting for native JSON/UTF-16 representations.
//! Permission and final posting remain with the actual caller/Registry fence.

#[cfg(test)]
use super::CorrelationId;
use super::{
    Arc, CompatLedger, CompatSession, ControlCharge, HostAppError, MAX_CONTROL_BYTES, Mutex,
    Ordering, PrimaryCallerContext, app_detail,
};
use keld_wv::renderer_bridge::{NativeControlCharge, NativeControlReservation};
use std::sync::atomic::AtomicBool;

pub(super) const CONTROL_ACCOUNTED_BYTES: usize = 65_536;
pub(super) const CONTROL_RECORD_BYTES: usize = 1024;

struct NativeRelease {
    released: Arc<AtomicBool>,
    // Retain the real same ledger while native storage survives role retirement.
    // No ledger lock is taken by Drop, including when a queued outcome is discarded.
    owner: Option<Arc<Mutex<CompatSession>>>,
    progress: Option<super::super::primary_writer::PrimaryWriter>,
}
impl NativeControlCharge for NativeRelease {
    fn try_extend(&self, encoded_bytes: usize, accounted_bytes: usize) -> bool {
        let Some(owner) = &self.owner else {
            return false;
        };
        let Ok(mut owner) = owner.lock() else {
            return false;
        };
        owner
            .ledger
            .extend_native_control(&self.released, encoded_bytes, accounted_bytes)
    }
    fn try_extend_storage(&self, encoded_bytes: usize, additional_accounted_bytes: usize) -> bool {
        let Some(owner) = &self.owner else {
            return false;
        };
        let Ok(mut owner) = owner.lock() else {
            return false;
        };
        owner.ledger.extend_native_storage(
            &self.released,
            encoded_bytes,
            additional_accounted_bytes,
        )
    }
}
impl Drop for NativeRelease {
    fn drop(&mut self) {
        // No ledger lock in a native/outcome destructor: send failure may drop
        // an outcome while Core already owns that same ledger lock. Only this
        // exact non-clone lease can report release. The actual ledger reaps it.
        self.released.store(true, Ordering::Release);
        if let Some(progress) = &self.progress {
            progress.native_control_progress();
        }
    }
}

impl ControlCharge {
    /// The same predicate serves reaping and read-only endpoint observation.
    /// A poisoned/expired observer cannot authorize storage release.
    pub(super) fn retained_obligations_quiescent(
        &self,
    ) -> Result<bool, super::super::primary_writer::WriterRefusal> {
        if !self.application_done
            || self
                .native_release
                .as_ref()
                .is_some_and(|released| !released.load(Ordering::Acquire))
        {
            return Ok(false);
        }
        self.primary_write.as_ref().map_or(
            Ok(true),
            super::super::primary_writer::WriteObservation::is_quiesced,
        )
    }
}

impl CompatLedger {
    pub(super) fn reap_native_controls(&mut self) {
        let mut released_any = false;
        for slot in &mut self.controls {
            if slot
                .as_ref()
                .is_some_and(|charge| charge.retained_obligations_quiescent().unwrap_or(false))
            {
                *slot = None;
                released_any = true;
            }
        }
        if released_any && let Some(progress) = &self.control_progress {
            progress.native_control_progress();
        }
    }

    pub(super) fn control_accounted_bytes(&self) -> usize {
        self.controls
            .iter()
            .flatten()
            .map(|charge| {
                // Existing app-link control callers reserve the payload maximum;
                // native reservations already include this fixed metadata charge.
                charge.retained
                    + if charge.native_release.is_none() {
                        CONTROL_RECORD_BYTES
                    } else {
                        0
                    }
            })
            .sum()
    }

    fn extend_native_storage(
        &mut self,
        released: &Arc<AtomicBool>,
        encoded_bytes: usize,
        additional_accounted_bytes: usize,
    ) -> bool {
        let retained = self
            .controls
            .iter()
            .flatten()
            .find(|charge| {
                charge
                    .native_release
                    .as_ref()
                    .is_some_and(|original| Arc::ptr_eq(original, released))
            })
            .map(|charge| charge.retained);
        let Some(total) =
            retained.and_then(|retained| retained.checked_add(additional_accounted_bytes))
        else {
            return false;
        };
        self.extend_native_control(released, encoded_bytes, total)
    }

    fn extend_native_control(
        &mut self,
        released: &Arc<AtomicBool>,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> bool {
        self.reap_native_controls();
        if encoded_bytes == 0
            || encoded_bytes > MAX_CONTROL_BYTES
            || accounted_bytes < encoded_bytes.saturating_add(CONTROL_RECORD_BYTES)
            || released.load(Ordering::Acquire)
        {
            return false;
        }
        let accounted = self.control_accounted_bytes();
        let Some(charge) = self.controls.iter_mut().flatten().find(|charge| {
            charge
                .native_release
                .as_ref()
                .is_some_and(|original| Arc::ptr_eq(original, released))
        }) else {
            return false;
        };
        let next = accounted_bytes.max(charge.retained);
        if accounted
            .saturating_sub(charge.retained)
            .saturating_add(next)
            > CONTROL_ACCOUNTED_BYTES
        {
            return false;
        }
        charge.encoded = encoded_bytes.max(charge.encoded);
        charge.retained = next;
        true
    }

    /// Move native custody onto an existing app-link correlation. The app-link
    /// terminal and native completion must BOTH release this exact original slot.
    #[cfg(test)]
    fn lease_existing_control(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: u32,
        additional_owned_bytes: usize,
        owner: Option<Arc<Mutex<CompatSession>>>,
    ) -> Option<NativeControlReservation> {
        self.lease_existing_control_scoped(caller, correlation, additional_owned_bytes, owner, None)
    }

    fn lease_existing_control_scoped(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: u32,
        additional_owned_bytes: usize,
        owner: Option<Arc<Mutex<CompatSession>>>,
        window: Option<keld_ipc::window::WindowRef>,
    ) -> Option<NativeControlReservation> {
        self.reap_native_controls();
        let index = self.controls.iter().position(|slot| {
            slot.as_ref().is_some_and(|charge| {
                charge.owned_by(caller)
                    && charge.correlation == correlation
                    && !charge.application_done
                    && charge.native_release.is_none()
            })
        })?;
        if self
            .control_accounted_bytes()
            .checked_add(additional_owned_bytes)?
            > CONTROL_ACCOUNTED_BYTES
        {
            return None;
        }
        let charge = self.controls[index].as_mut()?;
        let accounted = charge
            .retained
            .checked_add(CONTROL_RECORD_BYTES)?
            .checked_add(additional_owned_bytes)?;
        if charge
            .native_window
            .is_some_and(|previous| Some(previous) != window)
        {
            return None;
        }
        charge.native_window = window;
        charge.retained = accounted; // reserve all simultaneous storage before lease allocation
        let released = Arc::new(AtomicBool::new(false));
        charge.native_release = Some(Arc::clone(&released));
        Some(NativeControlReservation::from_owner(NativeRelease {
            released,
            owner,
            progress: self.control_progress.clone(),
        }))
    }

    #[cfg(test)]
    fn reserve_native_control(
        &mut self,
        caller: &PrimaryCallerContext,
        encoded_bytes: usize,
        accounted_bytes: usize,
        owner: Option<Arc<Mutex<CompatSession>>>,
    ) -> Option<NativeControlReservation> {
        self.reserve_native_control_scoped(caller, encoded_bytes, accounted_bytes, owner, None)
    }

    fn reserve_native_control_scoped(
        &mut self,
        caller: &PrimaryCallerContext,
        encoded_bytes: usize,
        accounted_bytes: usize,
        owner: Option<Arc<Mutex<CompatSession>>>,
        window: Option<keld_ipc::window::WindowRef>,
    ) -> Option<NativeControlReservation> {
        self.reap_native_controls();
        if encoded_bytes == 0
            || encoded_bytes > MAX_CONTROL_BYTES
            || accounted_bytes < encoded_bytes.checked_add(CONTROL_RECORD_BYTES)?
            || self
                .control_accounted_bytes()
                .checked_add(accounted_bytes)?
                > CONTROL_ACCOUNTED_BYTES
        {
            return None;
        }
        let slot = self.controls.iter_mut().find(|slot| slot.is_none())?;
        // Fixed slot/byte accounting is charged before its release notification
        // and opaque lease allocations. Both fit the existing record allowance.
        *slot = Some(ControlCharge {
            owner: caller.clone(),
            correlation: 0,
            retained: accounted_bytes,
            native_release: None,
            application_done: true,
            encoded: encoded_bytes,
            primary_write: None,
            native_window: window,
        });
        let released = Arc::new(AtomicBool::new(false));
        slot.as_mut()?.native_release = Some(Arc::clone(&released));
        Some(NativeControlReservation::from_owner(NativeRelease {
            released,
            owner,
            progress: self.control_progress.clone(),
        }))
    }
}

impl CompatSession {
    /// Bind the one application-session primary writer's existing coalesced wake
    /// owner before any native CONTROL can outlive a creating window operation.
    pub(in crate::app_session) fn bind_primary_writer(
        &mut self,
        writer: super::super::primary_writer::PrimaryWriter,
    ) -> Result<(), HostAppError> {
        if self.ledger.control_progress.is_some() {
            return Err(app_detail(
                "compat CONTROL progress",
                "the session writer is already bound",
            ));
        }
        self.ledger.control_progress = Some(writer);
        Ok(())
    }

    /// Called with the actual ledger held after Full. Register then reap/recheck
    /// so a native Drop racing this cut cannot leave a retained record asleep.
    #[cfg(test)]
    pub(in crate::app_session) fn register_native_control_interest(
        &mut self,
    ) -> Result<(), HostAppError> {
        let writer = self.ledger.control_progress.clone().ok_or_else(|| {
            app_detail("compat CONTROL progress", "session writer is unavailable")
        })?;
        writer
            .register_native_control_interest()
            .map_err(|error| error.into_host_error("compat CONTROL interest"))?;
        self.ledger.reap_native_controls();
        if self.ledger.controls.iter().any(Option::is_none) {
            writer.native_control_progress();
        }
        Ok(())
    }

    /// Existing correlation keeps its app-link half until its physical writer
    /// completes; native storage carries the other half through the UI endpoint.
    #[cfg(test)]
    #[cfg_attr(
        all(windows, test),
        expect(
            dead_code,
            reason = "Retain the historical session-level CONTROL adapter for the test-compiled legacy path; current Windows controls exercise the same ledger directly."
        )
    )]
    pub(super) fn lease_existing_control(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: u32,
        additional_owned_bytes: usize,
        owner: Arc<Mutex<Self>>,
    ) -> Option<NativeControlReservation> {
        self.ledger.control_progress.as_ref()?;
        self.ledger
            .lease_existing_control(caller, correlation, additional_owned_bytes, Some(owner))
    }

    /// Caller already holds the exact canonical live or narrow retirement fence.
    #[cfg(test)]
    pub(in crate::app_session) fn reserve_native_control(
        &mut self,
        caller: &PrimaryCallerContext,
        encoded_bytes: usize,
        accounted_bytes: usize,
        owner: Arc<Mutex<Self>>,
    ) -> Option<NativeControlReservation> {
        self.ledger.control_progress.as_ref()?;
        self.ledger
            .reserve_native_control(caller, encoded_bytes, accounted_bytes, Some(owner))
    }
}

const _: () = assert!(
    std::mem::size_of::<ControlCharge>()
        + std::mem::size_of::<NativeRelease>()
        + std::mem::size_of::<NativeControlReservation>()
        + 4 * std::mem::size_of::<usize>()
        <= CONTROL_RECORD_BYTES
);

#[cfg(test)]
mod tests;

impl CompatSession {
    /// The existing caller/Registry cut must have checked this exact physical
    /// Entry's open-offer state. This only charges its SAME storage record.
    pub(in crate::app_session) fn reserve_native_control_for_window(
        &mut self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
        encoded_bytes: usize,
        accounted_bytes: usize,
        owner: Arc<Mutex<Self>>,
    ) -> Option<NativeControlReservation> {
        self.ledger.control_progress.as_ref()?;
        self.ledger.reserve_native_control_scoped(
            caller,
            encoded_bytes,
            accounted_bytes,
            Some(owner),
            Some(window),
        )
    }
    pub(super) fn lease_existing_control_for_window(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: u32,
        window: keld_ipc::window::WindowRef,
        additional_owned_bytes: usize,
        owner: Arc<Mutex<Self>>,
    ) -> Option<NativeControlReservation> {
        self.ledger.control_progress.as_ref()?;
        self.ledger.lease_existing_control_scoped(
            caller,
            correlation,
            additional_owned_bytes,
            Some(owner),
            Some(window),
        )
    }
}
