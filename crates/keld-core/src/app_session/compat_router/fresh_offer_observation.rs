//! Fixed actual CONTROL census for the X10 live closed-producer challenge.
//! The caller holds the ledger. Only scalars leave it; no lease/flag is cloned.
#![forbid(unsafe_code)]

use super::CompatSession;
use super::native_control::{CONTROL_ACCOUNTED_BYTES, CONTROL_RECORD_BYTES};
use keld_ipc::window::WindowRef;
use keld_wv::renderer_bridge::NativeControlReservation;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct ControlRow {
    slot: usize,
    window: Option<WindowRef>,
    retained: usize,
    encoded: usize,
    native_release_id: usize,
    native_released: Option<bool>,
    application_done: bool,
    primary_write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(in crate::app_session) struct ControlCensus {
    controls: [Option<ControlRow>; 4],
    accounted_bytes: usize,
    slot_limit: usize,
    byte_limit: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(in crate::app_session) struct ReleaseProof {
    held: ControlCensus,
    dropped: ControlCensus,
    returned: ControlCensus,
}

// The valid probe has one encoded byte and the same fixed owner metadata as an
// ordinary native reservation. It never creates a payload or sends a native call.
pub(in crate::app_session) const PROBE_ENCODED: usize = 1;
pub(in crate::app_session) const PROBE_ACCOUNTED: usize = CONTROL_RECORD_BYTES + PROBE_ENCODED;

impl CompatSession {
    fn fresh_offer_census(&self) -> ControlCensus {
        ControlCensus {
            controls: std::array::from_fn(|slot| {
                let charge = self.ledger.controls[slot].as_ref()?;
                Some(ControlRow {
                    slot,
                    window: charge.native_window,
                    retained: charge.retained,
                    encoded: charge.encoded,
                    // Passive identity of the actual original allocation only.
                    // Never dereferenced or used for routing/admission.
                    native_release_id: charge
                        .native_release
                        .as_ref()
                        .map_or(0, |released| Arc::as_ptr(released) as usize),
                    native_released: charge
                        .native_release
                        .as_ref()
                        .map(|released| released.load(Ordering::Acquire)),
                    application_done: charge.application_done,
                    primary_write: charge.primary_write.is_some(),
                })
            }),
            accounted_bytes: self.ledger.control_accounted_bytes(),
            slot_limit: self.ledger.controls.len(),
            byte_limit: CONTROL_ACCOUNTED_BYTES,
        }
    }

    /// Normalize only already-quiescent rows using the actual owner predicate.
    /// No fresh completion is asserted by this test-only scheduling operation.
    pub(in crate::app_session) fn fresh_offer_baseline(&mut self) -> ControlCensus {
        self.ledger.reap_native_controls();
        self.fresh_offer_census()
    }

    /// The real non-clone token is dropped under the ledger guard so its release
    /// can be sampled before a concurrent reaper removes the original record.
    /// `NativeRelease::drop` explicitly takes no ledger lock. The ordinary reap
    /// predicate then returns the slot/bytes; this hook never clears a slot itself.
    pub(in crate::app_session) fn finish_fresh_offer_reservation(
        &mut self,
        reservation: Option<NativeControlReservation>,
    ) -> ReleaseProof {
        let held = self.fresh_offer_census();
        drop(reservation);
        let dropped = self.fresh_offer_census();
        self.ledger.reap_native_controls();
        let returned = self.fresh_offer_census();
        ReleaseProof {
            held,
            dropped,
            returned,
        }
    }
}
