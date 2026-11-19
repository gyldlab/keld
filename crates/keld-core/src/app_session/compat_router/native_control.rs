//! The fixed GH540 CONTROL records retain native representations through
//! actual disposal, including across Runtime role replacement.

use super::CompatSession;
use keld_ipc::compat_control::COMPAT_CONTROL_FRAME_BYTES;
use keld_ipc::window::WindowRef;
use keld_wv::renderer_bridge::{NativeControlCharge, NativeControlReservation};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const CONTROL_RECORDS: usize = 4;
const CONTROL_ACCOUNTED_BYTES: usize = 65_536;
const CONTROL_RECORD_BYTES: usize = 1024;

struct ControlCharge {
    window: WindowRef,
    accounted: usize,
    released: Option<Arc<AtomicBool>>,
}

impl ControlCharge {
    fn is_released(&self) -> bool {
        self.released
            .as_ref()
            .is_some_and(|released| released.load(Ordering::Acquire))
    }
}

#[derive(Default)]
pub(super) struct CompatLedger {
    controls: [Option<ControlCharge>; CONTROL_RECORDS],
}

impl CompatLedger {
    fn reap_native_controls(&mut self) {
        for slot in &mut self.controls {
            if slot.as_ref().is_some_and(ControlCharge::is_released) {
                *slot = None;
            }
        }
    }

    fn control_accounted_bytes(&self) -> usize {
        self.controls
            .iter()
            .flatten()
            .map(|charge| charge.accounted)
            .sum()
    }
}

fn valid_control_storage(encoded: usize, accounted: usize) -> bool {
    (1..=COMPAT_CONTROL_FRAME_BYTES).contains(&encoded)
        && accounted >= encoded + CONTROL_RECORD_BYTES
}

struct NativeRelease {
    // Retain this exact session ledger even if the physical view outlives its
    // Runtime caller. Drop signals the original flag without locking it.
    owner: Arc<CompatSession>,
    released: Arc<AtomicBool>,
}

impl NativeRelease {
    fn try_grow(
        &self,
        encoded: usize,
        accounted_size: impl FnOnce(usize) -> Option<usize>,
    ) -> Option<()> {
        let mut ledger = self.owner.ledger.lock().ok()?;
        ledger.reap_native_controls();
        let total = ledger.control_accounted_bytes();
        let charge = ledger.controls.iter_mut().flatten().find(|charge| {
            charge
                .released
                .as_ref()
                .is_some_and(|released| Arc::ptr_eq(released, &self.released))
        })?;
        let requested = accounted_size(charge.accounted)?;
        let next = requested.max(charge.accounted);
        if !valid_control_storage(encoded, requested)
            || (total - charge.accounted).checked_add(next)? > CONTROL_ACCOUNTED_BYTES
        {
            return None;
        }
        charge.accounted = next;
        Some(())
    }
}

impl NativeControlCharge for NativeRelease {
    fn try_extend(&self, encoded_bytes: usize, accounted_bytes: usize) -> bool {
        self.try_grow(encoded_bytes, |_| Some(accounted_bytes))
            .is_some()
    }

    fn try_extend_storage(&self, encoded_bytes: usize, additional_accounted_bytes: usize) -> bool {
        self.try_grow(encoded_bytes, |retained| {
            retained.checked_add(additional_accounted_bytes)
        })
        .is_some()
    }
}

impl Drop for NativeRelease {
    fn drop(&mut self) {
        // Outcome disposal may already hold the ledger. Wake the existing
        // servicing owner without taking that lock or refunding a writer slot.
        self.released.store(true, Ordering::Release);
        let _ = self.owner.writer.completed_source_progress();
    }
}

impl CompatSession {
    /// Caller holds the canonical Runtime and exact open Window Registry cut.
    pub(in crate::app_session) fn reserve_native_control(
        self: &Arc<Self>,
        window: WindowRef,
        encoded: usize,
        accounted: usize,
    ) -> Option<NativeControlReservation> {
        let mut ledger = self.ledger.lock().ok()?;
        ledger.reap_native_controls();
        if !valid_control_storage(encoded, accounted)
            || ledger.control_accounted_bytes().checked_add(accounted)? > CONTROL_ACCOUNTED_BYTES
        {
            return None;
        }
        let slot = ledger.controls.iter_mut().find(|record| record.is_none())?;
        let charge = slot.insert(ControlCharge {
            window,
            accounted,
            released: None,
        });
        let released = Arc::new(AtomicBool::new(false));
        charge.released = Some(Arc::clone(&released));
        Some(NativeControlReservation::from_owner(NativeRelease {
            owner: Arc::clone(self),
            released,
        }))
    }

    /// This is a physical backing observation, independent of current role.
    /// The Registry must first prove that this physical producer has closed.
    pub(in crate::app_session) fn native_storage_is_quiescent(&self, window: WindowRef) -> bool {
        self.ledger.lock().is_ok_and(|ledger| {
            ledger
                .controls
                .iter()
                .flatten()
                .all(|charge| charge.window != window || charge.is_released())
        })
    }
}

const _: () = assert!(
    std::mem::size_of::<ControlCharge>()
        + std::mem::size_of::<NativeRelease>()
        + std::mem::size_of::<NativeControlReservation>()
        + std::mem::size_of::<AtomicBool>()
        + 2 * std::mem::size_of::<usize>()
        <= CONTROL_RECORD_BYTES
);
