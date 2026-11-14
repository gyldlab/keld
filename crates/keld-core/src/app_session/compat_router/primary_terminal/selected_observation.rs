//! X10 scalar observations of the original selected diagnostic and writer slot.
//! No diagnostic text, Box, Arc, lease, receipt or replacement ledger is copied.
#![forbid(unsafe_code)]

use super::{Charge, CompatSession, ConsumerDisposition, PrimaryRouterHandle, TerminalSource};
use crate::app_session::HostAppError;
use crate::app_session::compat_router::{
    CompatControlResponse, Direction, NativeRouteKey, TerminalDisposition,
};
use crate::app_session::primary_writer::PrimaryWriter;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(in crate::app_session) struct SelectedDiagnostic {
    webview: u32,
    navigation: u32,
    message: u32,
    reader_stop_id: usize,
    canonical_code: bool,
    code_id: usize,
    code_bytes: usize,
    message_id: usize,
    message_bytes: usize,
}

impl SelectedDiagnostic {
    fn from_error(key: NativeRouteKey, reader_stop_id: usize, error: &keld_ipc::CallError) -> Self {
        Self {
            webview: key.webview,
            navigation: key.navigation,
            message: key.message,
            reader_stop_id,
            canonical_code: error.code == "KELD-COMPAT-103",
            code_id: error.code.as_ptr() as usize,
            code_bytes: error.code.len(),
            message_id: error.message.as_ptr() as usize,
            message_bytes: error.message.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app_session) struct SelectedPreparation {
    diagnostic: SelectedDiagnostic,
    attempt: u32,
}

pub(super) fn source_attempt(charge: &Charge) -> Option<u32> {
    charge
        .terminal_preparing
        .then_some(charge.selected_source_sequence)
        .filter(|attempt| (1..=128).contains(attempt))
}

pub(super) struct RestoredSelectedSource {
    preparation: SelectedPreparation,
    charges: [Option<ChargeSnapshot>; 2],
}

impl TerminalSource {
    pub(super) fn selected_diagnostic(&self) -> Option<SelectedPreparation> {
        let CompatControlResponse::Deliver {
            disposition: TerminalDisposition::Refused { error },
        } = self.response.as_ref()?
        else {
            return None;
        };
        Some(SelectedPreparation {
            diagnostic: SelectedDiagnostic::from_error(
                self.key,
                Arc::as_ptr(&self.caller.reader_stop) as usize,
                error,
            ),
            attempt: self.source_attempt?,
        })
    }

    pub(super) fn finish_denied_target(mut self) -> Option<RestoredSelectedSource> {
        let preparation = self.selected_diagnostic();
        let mut restored = None;
        // Reuse the actual Drop implementation. Capture only after both restore
        // and the unchanged quiescent release attempt, under the same guard.
        self.drop_restoration_observed = true;
        self.restore_on_drop(|state| {
            if let Some(preparation) = preparation {
                restored = Some(RestoredSelectedSource {
                    preparation,
                    charges: charge_snapshots(state),
                });
            }
        });
        drop(self); // fields drop now; the same restoration body has already run exactly once
        restored
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Each scalar preserves an independent original source/native/admission obligation."
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct ChargeSnapshot {
    webview: u32,
    navigation: u32,
    message: u32,
    reader_stop_id: usize,
    reader_stopped: bool,
    retained: usize,
    selected: Option<SelectedDiagnostic>,
    pending_reply: Option<u32>,
    terminal_preparing: bool,
    source_attempt: Option<u32>,
    primary_write: bool,
    retirement_write: bool,
    consumer_done: bool,
    native_done: bool,
    retired: bool,
    native_retirement: Option<&'static str>,
    native_storage_id: Option<usize>,
    native_storage_released: Option<bool>,
    event_release_sent: bool,
}

impl ChargeSnapshot {
    fn from_charge(charge: &Charge) -> Self {
        let reader_stop_id = Arc::as_ptr(&charge.owner.reader_stop) as usize;
        let selected =
            match &charge.terminal {
                Some(ConsumerDisposition::Refused { error }) => Some(
                    SelectedDiagnostic::from_error(charge.key, reader_stop_id, error),
                ),
                _ => None,
            };
        Self {
            webview: charge.key.webview,
            navigation: charge.key.navigation,
            message: charge.key.message,
            reader_stop_id,
            reader_stopped: charge.owner.reader_stop.load(Ordering::Acquire),
            retained: charge.retained,
            selected,
            pending_reply: charge.pending_reply.map(|value| value.0),
            terminal_preparing: charge.terminal_preparing,
            source_attempt: source_attempt(charge),
            primary_write: charge.primary_write.is_some(),
            retirement_write: charge.retirement_write.is_some(),
            consumer_done: charge.consumer_done,
            native_done: charge.native_done,
            retired: charge.retired,
            native_retirement: charge.native_retirement.map(|certainty| match certainty {
                keld_ipc::compat_control::DispatchCertainty::NotDispatched => "not-dispatched",
                keld_ipc::compat_control::DispatchCertainty::OutcomeUnknown => "outcome-unknown",
            }),
            native_storage_id: charge
                .native_storage
                .as_ref()
                .map(|value| Arc::as_ptr(value) as usize),
            native_storage_released: charge
                .native_storage
                .as_ref()
                .map(|value| value.load(Ordering::Acquire)),
            event_release_sent: charge.event_release_sent,
        }
    }
}

fn charge_snapshots(state: &CompatSession) -> [Option<ChargeSnapshot>; 2] {
    [
        state.ledger.data[Direction::RendererToMain.index()]
            .as_ref()
            .map(ChargeSnapshot::from_charge),
        state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .map(ChargeSnapshot::from_charge),
    ]
}

fn enabled() -> bool {
    std::env::var_os(super::super::super::primary_writer::TEST_PRIMARY_VALUE_HOLD_ENV).is_some()
        && std::env::var_os(keld_wv::TEST_WINDOWS_T2_OBSERVE_ENV).as_deref()
            == Some(std::ffi::OsStr::new("1"))
}

impl PrimaryRouterHandle {
    /// Compare the actual returned error before cleanup aggregation. Retained
    /// source state and unrelated errors cannot grant a selected refusal.
    pub(in crate::app_session) fn observed_selected_retirement_refusal(
        error: &HostAppError,
    ) -> Option<&'static str> {
        match (error.code, error.phase, error.detail.as_str()) {
            (
                "KELD-CORE-037",
                "compat retirement snapshot",
                "selected original diagnostic/source cannot be replaced by a fixed retirement",
            ) => Some("selected-diagnostic"),
            (
                "KELD-CORE-037",
                "compat retirement",
                "selected original terminal remains retained; no replacement terminal is permitted",
            ) => Some("selected-terminal"),
            _ => None,
        }
    }

    /// Call only after existing canonical/source guards end. The worker hold,
    /// not these separately sampled values, supplies the scheduling barrier.
    pub(in crate::app_session) fn observe_selected_diagnostic(
        &self,
        phase: &'static str,
        selected: Option<SelectedPreparation>,
        value_full: bool,
        retirement_failed: Option<bool>,
        retirement_refusal: Option<&'static str>,
    ) {
        if !enabled() {
            return;
        }
        let charges = {
            let Some(compat) = &self.compat else { return };
            let Ok(state) = compat.lock() else { return };
            charge_snapshots(&state)
        };
        self.emit_selected_diagnostic(
            phase,
            selected,
            &charges,
            value_full,
            retirement_failed,
            retirement_refusal,
        );
    }

    pub(super) fn observe_restored_selected_source(&self, restored: &RestoredSelectedSource) {
        // The exact denied preparation has restored and dropped. Do not take a
        // later ledger snapshot: another progress turn may already own its strings.
        self.emit_selected_diagnostic(
            "source-restored-after-denied-target",
            Some(restored.preparation),
            &restored.charges,
            false,
            None,
            None,
        );
    }

    fn emit_selected_diagnostic(
        &self,
        phase: &'static str,
        selected: Option<SelectedPreparation>,
        charges: &[Option<ChargeSnapshot>; 2],
        value_full: bool,
        retirement_failed: Option<bool>,
        retirement_refusal: Option<&'static str>,
    ) {
        type Record = (
            &'static str,
            Option<SelectedPreparation>,
            [Option<ChargeSnapshot>; 2],
            bool,
            Option<bool>,
            Option<&'static str>,
        );
        static LAST: Mutex<(usize, Option<Record>)> = Mutex::new((0, None));
        if !enabled() {
            return;
        }
        let record = (
            phase,
            selected,
            *charges,
            value_full,
            retirement_failed,
            retirement_refusal,
        );
        let sequence = {
            let Ok(mut last) = LAST.lock() else { return };
            if last.0 >= 128 || last.1 == Some(record) {
                return;
            }
            last.0 += 1;
            last.1 = Some(record);
            last.0
        };
        let position = self
            .primary_writer
            .as_ref()
            .and_then(PrimaryWriter::observed_value_position);
        eprintln!(
            "KELD_X10_SELECTED_DIAGNOSTIC {}",
            serde_json::json!({
                "sequence": sequence, "phase": phase,
                "selected": selected.map(|value| value.diagnostic),
                "source_attempt": selected.map(|value| value.attempt),
                "renderer_to_main": charges[0], "main_to_renderer": charges[1],
                "value_position": position, "value_full": value_full, "retirement_failed": retirement_failed,
                "retirement_refusal": retirement_refusal,
            })
        );
    }
}

#[cfg(test)]
mod tests;
