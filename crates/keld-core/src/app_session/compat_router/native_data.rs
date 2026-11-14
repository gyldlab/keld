//! Native event backing extends the SAME existing DATA Charge. The token owns
//! storage only; native return/FunctionFinished and consumer receipts stay distinct.
#[cfg(all(test, debug_assertions))]
use super::CorrelationId;
use super::{
    Arc, Charge, CompatLedger, CompatSession, Direction, HostAppError,
    MAX_ALL_CONTROL_RETAINED_BYTES, MAX_DATA_RETAINED_BYTES, MacNativeDocumentLease, Mutex,
    NativeRouteKey, Ordering, PrimaryCallerContext, RendererBridgeOutcome, RendererRoleWitness,
    WebviewId,
};
use keld_wv::renderer_bridge::{NativeDataCharge, NativeDataReservation};
use std::sync::atomic::AtomicBool;

// Included within MAX_DATA_RETAINED_BYTES, not an additional allowance.
pub(super) const DATA_STORAGE_METADATA_BYTES: usize = 1024;

struct NativeDataRelease {
    released: Arc<AtomicBool>,
    owner: Arc<Mutex<CompatSession>>,
    progress: Option<super::super::primary_writer::PrimaryWriter>,
}
impl NativeDataCharge for NativeDataRelease {
    fn try_extend_storage(&self, encoded_bytes: usize, additional_accounted_bytes: usize) -> bool {
        let Ok(mut state) = self.owner.lock() else {
            return false;
        };
        state.ledger.extend_native_data_storage(
            &self.released,
            encoded_bytes,
            additional_accounted_bytes,
        )
    }
}
impl Drop for NativeDataRelease {
    fn drop(&mut self) {
        // Actual storage is gone before this final token field is dropped. Never
        // lock the same ledger here: a never-enqueued outcome can drop under it.
        self.released.store(true, Ordering::Release);
        if let Some(progress) = &self.progress {
            progress.native_control_progress();
            // DATA completion must service its retained record even when no
            // CONTROL capacity interest was registered. Reuse the same wake.
            let _ = progress.completed_source_progress();
        }
    }
}

impl CompatLedger {
    fn extend_native_data_storage(
        &mut self,
        released: &Arc<AtomicBool>,
        encoded: usize,
        additional: usize,
    ) -> bool {
        if encoded == 0
            || encoded > keld_wv::wv_link::MAX_WV_LINK_ENVELOPE_LEN
            || released.load(Ordering::Acquire)
        {
            return false;
        }
        let Some(aggregate) = self.retained_bytes().checked_add(additional) else {
            return false;
        };
        if aggregate > 2 * MAX_DATA_RETAINED_BYTES + MAX_ALL_CONTROL_RETAINED_BYTES {
            return false;
        }
        let Some(charge) = self.data.iter_mut().flatten().find(|charge| {
            charge
                .native_storage
                .as_ref()
                .is_some_and(|held| Arc::ptr_eq(held, released))
        }) else {
            return false;
        };
        let Some(total) = charge.retained.checked_add(additional) else {
            return false;
        };
        if total > MAX_DATA_RETAINED_BYTES || total < encoded {
            return false;
        }
        charge.retained = total;
        true
    }
}

impl CompatLedger {
    /// Called at the actual DATA admission cut after reserving this exact record.
    /// No replacement record, selector-only lease or permission is constructed.
    pub(super) fn lease_native_data(
        &mut self,
        caller: &PrimaryCallerContext,
        key: NativeRouteKey,
        document: &MacNativeDocumentLease,
        owner: Arc<Mutex<CompatSession>>,
    ) -> Option<NativeDataReservation> {
        let progress = self.control_progress.as_ref()?.clone();
        let aggregate = self
            .retained_bytes()
            .checked_add(DATA_STORAGE_METADATA_BYTES)?;
        if aggregate > 2 * MAX_DATA_RETAINED_BYTES + MAX_ALL_CONTROL_RETAINED_BYTES {
            return None;
        }
        let charge = self.data[Direction::MainToRenderer.index()].as_mut()?;
        charge.native_window?; // actual final route supplied the physical Entry
        if charge.key != key
            || !charge.owned_by(caller)
            || charge.native_storage.is_some()
            || !charge
                .document
                .as_ref()
                .is_some_and(|held| held.same_binding(document))
        {
            return None;
        }
        let total = charge.retained.checked_add(DATA_STORAGE_METADATA_BYTES)?;
        if total > MAX_DATA_RETAINED_BYTES {
            return None;
        }
        charge.retained = total; // all metadata charged before Arc/opaque token allocation
        let released = Arc::new(AtomicBool::new(false));
        charge.native_storage = Some(Arc::clone(&released));
        Some(NativeDataReservation::from_owner(NativeDataRelease {
            released,
            owner,
            progress: Some(progress),
        }))
    }
}

impl CompatSession {
    /// Exactly-once control on the existing endpoint after actual prerequisites.
    /// Sending this does not release storage: its same token reports that later.
    pub(super) fn release_completed_native_event(&mut self) -> Result<(), HostAppError> {
        let Some(charge) = self.ledger.data[Direction::MainToRenderer.index()].as_ref() else {
            return Ok(());
        };
        if !charge.event_obligations_settled()
            || charge.terminal_preparing
            || charge.primary_write.is_some()
            || charge.retirement_write.is_some()
            || charge.pending_reply.is_some()
        {
            return Ok(());
        }
        let key = charge.key;
        if !charge.event_release_sent {
            self.send(RendererBridgeOutcome::EventReleased {
                webview: WebviewId(key.webview),
                navigation: u64::from(key.navigation),
                request: key.message,
            })?;
            // Same ledger guard; no fallible work between successful send/commit.
            if let Some(charge) = &mut self.ledger.data[Direction::MainToRenderer.index()] {
                charge.event_release_sent = true;
            }
        }
        self.ledger.clear_if_complete(Direction::MainToRenderer);
        Ok(())
    }
}

const _: () = assert!(
    std::mem::size_of::<Charge>()
        + std::mem::size_of::<NativeDataRelease>()
        + std::mem::size_of::<NativeDataReservation>()
        + super::super::renderer_role::NATIVE_ROLE_FENCE_BACKING_BYTES
        + std::mem::size_of::<RendererRoleWitness>()
        + std::mem::size_of::<MacNativeDocumentLease>()
        + 4 * std::mem::size_of::<usize>()
        <= DATA_STORAGE_METADATA_BYTES
);

#[cfg(all(test, debug_assertions))]
mod tests;
