//! Passive fixed census of original Entry DATA/CONTROL storage for X10.
//! No payload, diagnostic string, lease or owner is copied; guards end before IO.
#![forbid(unsafe_code)]

use super::{CompatSession, Direction, PrimaryRouterHandle};
use keld_ipc::{CorrelationId, window::WindowRef};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[expect(
    clippy::struct_excessive_bools,
    reason = "Passive scalars preserve distinct existing native, consumer, retirement and storage facts."
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct DataSnapshot {
    direction: usize,
    webview: u32,
    navigation: u32,
    message: u32,
    retained: usize,
    native_release_id: usize,
    native_released: bool,
    consumer_done: bool,
    native_done: bool,
    event_release_sent: bool,
    retired: bool,
    native_retirement: bool,
    pending_reply: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct ControlSnapshot {
    slot: usize,
    retained: usize,
    encoded: usize,
    native_release_id: usize,
    native_released: bool,
    application_done: bool,
    primary_write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(in crate::app_session) struct ScopeSnapshot {
    data: [Option<DataSnapshot>; 2],
    controls: [Option<ControlSnapshot>; 4],
    unknown_native_data: usize,
    unknown_native_controls: usize,
    native_storage_quiescent: bool,
}

impl CompatSession {
    /// Caller holds the same ledger; only a fixed scalar value leaves that guard.
    pub(in crate::app_session) fn queued_close_snapshot(&self, window: WindowRef) -> ScopeSnapshot {
        // Native-release flags only advance false -> true while this ledger is
        // held. Sampling the predicate first keeps a true snapshot conservative.
        let native_storage_quiescent = self.native_storage_is_quiescent_for(window);
        ScopeSnapshot {
            data: std::array::from_fn(|index| {
                let charge = self.ledger.data[index].as_ref()?;
                if charge.native_window != Some(window) {
                    return None;
                }
                let released = charge.native_storage.as_ref()?;
                Some(DataSnapshot {
                    direction: index,
                    webview: charge.key.webview,
                    navigation: charge.key.navigation,
                    message: charge.key.message,
                    retained: charge.retained,
                    // Only an observation of the existing allocation identity.
                    // It is never dereferenced or used for authority/routing.
                    native_release_id: Arc::as_ptr(released) as usize,
                    native_released: released.load(Ordering::Acquire),
                    consumer_done: charge.consumer_done,
                    native_done: charge.native_done,
                    event_release_sent: charge.event_release_sent,
                    retired: charge.retired,
                    native_retirement: charge.native_retirement.is_some(),
                    pending_reply: charge.pending_reply.map(|value| value.0),
                })
            }),
            controls: std::array::from_fn(|slot| {
                let charge = self.ledger.controls[slot].as_ref()?;
                if charge.native_window != Some(window) {
                    return None;
                }
                let released = charge.native_release.as_ref()?;
                Some(ControlSnapshot {
                    slot,
                    retained: charge.retained,
                    encoded: charge.encoded,
                    native_release_id: Arc::as_ptr(released) as usize,
                    native_released: released.load(Ordering::Acquire),
                    application_done: charge.application_done,
                    primary_write: charge.primary_write.is_some(),
                })
            }),
            unknown_native_data: self
                .ledger
                .data
                .iter()
                .flatten()
                .filter(|charge| charge.native_storage.is_some() && charge.native_window.is_none())
                .count(),
            unknown_native_controls: self
                .ledger
                .controls
                .iter()
                .flatten()
                .filter(|charge| charge.native_release.is_some() && charge.native_window.is_none())
                .count(),
            native_storage_quiescent,
        }
    }
}

impl PrimaryRouterHandle {
    /// The real CALL admission has already ended all publication guards.
    pub(in crate::app_session) fn observe_queued_close_event_admission(
        &self,
        correlation: CorrelationId,
    ) {
        if !enabled() {
            return;
        }
        let selected = self.compat.as_ref().and_then(|compat| {
            let state = compat.lock().ok()?;
            let charge = state.ledger.data[Direction::MainToRenderer.index()].as_ref()?;
            // The actual Event key is the originating CALL correlation. Its
            // pending reply may already have settled on another thread, while
            // this same allocation remains retained behind the outcome hold.
            if charge.key.message != correlation.0 || charge.native_storage.is_none() {
                return None;
            }
            let window = charge.native_window?;
            Some((window, state.queued_close_snapshot(window)))
        });
        if let Some((window, snapshot)) = selected {
            observe("event-admitted", window, false, &snapshot);
        }
    }
}

fn enabled() -> bool {
    std::env::var_os(keld_wv::TEST_WINDOWS_T2_OBSERVE_ENV).as_deref()
        == Some(std::ffi::OsStr::new("1"))
}

/// All callers have released canonical guards. Bounded observer state contains
/// only fixed scalar snapshots, never a source or a new operation/receipt.
pub(in crate::app_session) fn observe(
    phase: &'static str,
    window: WindowRef,
    offers_closed: bool,
    snapshot: &ScopeSnapshot,
) {
    type Record = (WindowRef, &'static str, bool, ScopeSnapshot);
    static LAST: Mutex<(usize, Option<Record>)> = Mutex::new((0, None));
    if !enabled() {
        return;
    }
    let record = (window, phase, offers_closed, *snapshot);
    let sequence = {
        let Ok(mut last) = LAST.lock() else { return };
        if last.0 >= 128 || last.1 == Some(record) {
            return;
        }
        last.0 += 1;
        last.1 = Some(record);
        last.0
    };
    let value = serde_json::json!({
        "sequence": sequence, "phase": phase, "window": window.id.0,
        "incarnation": window.incarnation.0, "offers_closed": offers_closed,
        "scope": snapshot,
    });
    eprintln!("KELD_X10_NATIVE_SCOPE {value}");
}
