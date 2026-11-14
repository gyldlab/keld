//! X10 challenge on the exact live native scope, before and after real close.
//! No COM, pump, wait, new queue or completion flag is introduced.
#![forbid(unsafe_code)]

use super::super::{
    Arc, CanonicalRendererRoleFence, RendererRoleFence, with_primary_caller_locked,
};
use crate::app_session::compat_router::fresh_offer_observation::{
    ControlCensus, PROBE_ACCOUNTED, PROBE_ENCODED, ReleaseProof,
};
use keld_ipc::window::WindowRef;

#[derive(serde::Serialize)]
struct Attempt {
    caller_current: [bool; 2],
    scope_closed: [Option<bool>; 2],
    before: Option<ControlCensus>,
    admitted: bool,
    admission_calls: usize,
    reserved: bool,
    release: Option<ReleaseProof>,
}

/// Only fixed observations survive the probe. The temporary token is already
/// physically dropped/reaped before the original close code can run.
pub(super) struct Probe {
    window: WindowRef,
    caller_id: usize,
    before_close: Attempt,
}

fn enabled() -> bool {
    std::env::var(keld_wv::TEST_WINDOWS_NATIVE_HOLD_ENV)
        .ok()
        .and_then(|value| {
            value
                .split_once(':')
                .map(|(point, _)| point == "renderer-fresh-offers")
        })
        .unwrap_or(false)
}

impl CanonicalRendererRoleFence {
    fn fresh_offer_caller_current(&self) -> bool {
        let Some(current) = self.current.upgrade() else {
            return false;
        };
        let mut admitted = false;
        let result = with_primary_caller_locked(&current, &self.shutdown, &self.caller, |_| {
            admitted = true;
            Ok(())
        });
        result.is_ok() && admitted
    }

    fn fresh_offer_attempt(&self) -> Attempt {
        let caller_before = self.fresh_offer_caller_current();
        let scope_before = self.captured_view_scope_status();
        let before = self
            .compat
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .and_then(|compat| Some(compat.lock().ok()?.fresh_offer_baseline()));
        let mut admission_calls = 0;
        let admitted = self.try_admit_native(&mut || {
            admission_calls += 1;
            true
        });
        let reservation = self.reserve_control_for_captured_window(PROBE_ENCODED, PROBE_ACCOUNTED);
        let reserved = reservation.is_some();
        let release = self
            .compat
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .and_then(|compat| {
                Some(
                    compat
                        .lock()
                        .ok()?
                        .finish_fresh_offer_reservation(reservation),
                )
            });
        // Each helper has ended all guards. No lock or native reservation
        // survives these samples, the real close, or the later trace output.
        Attempt {
            caller_current: [caller_before, self.fresh_offer_caller_current()],
            scope_closed: [scope_before, self.captured_view_scope_status()],
            before,
            admitted,
            admission_calls,
            reserved,
            release,
        }
    }

    pub(super) fn begin_fresh_offer_probe(&self) -> Option<Probe> {
        if !enabled() || self.captured_view_scope_status() != Some(false) {
            return None;
        }
        Some(Probe {
            window: self.window?,
            // Observation only; canonical current checks above own authority.
            caller_id: Arc::as_ptr(&self.caller.reader_stop) as usize,
            before_close: self.fresh_offer_attempt(),
        })
    }
}

impl Probe {
    pub(super) fn finish(self, owner: &CanonicalRendererRoleFence) {
        let after_close = owner.fresh_offer_attempt();
        // At most one observation per actual open -> closed scope. Repeated
        // close sees the monotonic closed flag and cannot enter begin again.
        let value = serde_json::json!({
            "phase": "closed-producer-probed",
            "window": self.window.id.0,
            "incarnation": self.window.incarnation.0,
            "caller_id": self.caller_id,
            "encoded": PROBE_ENCODED,
            "accounted": PROBE_ACCOUNTED,
            "before_close": self.before_close,
            "after_close": after_close,
        });
        eprintln!("KELD_X10_FRESH_OFFER {value}");
    }
}
