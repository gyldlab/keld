//! Passive bounded scalars from the already selected original pressure owners.
//! No source, lease, receipt or owner is retained here.
#![forbid(unsafe_code)]

use super::super::{Charge, Direction};
use super::ScopeSnapshot;
use super::{Arc, Candidate, CompatSession, PressureRecord, PrimaryRouterHandle, SourceKey};
use crate::app_session::primary_writer::PrimaryWriter;
use keld_ipc::window::WindowRef;

#[derive(Clone, Copy)]
struct Selection {
    window: WindowRef,
    main: SourceKey,
    renderer: SourceKey,
    main_native_id: usize,
    caller_allocation: usize,
    control_ids: [usize; 4],
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Passive fields distinguish existing consumer, native, writer and retirement custody."
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct SourceState {
    key: SourceKey,
    caller_allocation: usize,
    retained: usize,
    consumer_done: bool,
    native_done: bool,
    primary_write: bool,
    event_release_sent: bool,
    retired: bool,
}

impl SourceState {
    fn capture(charge: &Charge) -> Self {
        Self {
            key: charge.key.into(),
            caller_allocation: Arc::as_ptr(&charge.owner.reader_stop) as usize,
            retained: charge.retained,
            consumer_done: charge.consumer_done,
            native_done: charge.native_done,
            primary_write: charge.primary_write.is_some(),
            event_release_sent: charge.event_release_sent,
            retired: charge.retired,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct Snapshot {
    main: Option<SourceState>,
    renderer: Option<SourceState>,
    controls_in_flight: usize,
    scope: ScopeSnapshot,
}

impl Snapshot {
    fn capture(state: &CompatSession, window: WindowRef) -> Self {
        Self {
            main: state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .map(SourceState::capture),
            renderer: state.ledger.data[Direction::RendererToMain.index()]
                .as_ref()
                .map(SourceState::capture),
            controls_in_flight: state.ledger.controls.iter().flatten().count(),
            scope: state.queued_close_snapshot(window),
        }
    }
}

pub(in crate::app_session::compat_router) struct PressureWatch {
    selection: Selection,
    last: Option<Snapshot>,
    sequence: u8,
}

impl PressureWatch {
    pub(super) fn new(selected: &Candidate, filled: &PressureRecord) -> Self {
        Self {
            selection: Selection {
                window: selected.route.window,
                main: filled.main,
                renderer: filled.renderer,
                main_native_id: filled.main_native_id,
                caller_allocation: filled.caller_allocation,
                control_ids: filled.control_ids,
            },
            last: None,
            sequence: 0,
        }
    }
}

#[derive(serde::Serialize)]
struct Progress<T> {
    phase: &'static str,
    cut: &'static str,
    sequence: u8,
    window: u32,
    incarnation: u64,
    main: SourceKey,
    renderer: SourceKey,
    main_native_id: usize,
    caller_allocation: usize,
    control_ids: [usize; 4],
    state: Snapshot,
    value_position: Option<T>,
}

impl PrimaryRouterHandle {
    /// Existing source-progress callers enter without canonical guards held.
    /// All ledger and writer guards end before serialization or output.
    pub(in crate::app_session) fn observe_x11_pressure_progress(&self, cut: &'static str) {
        let (selected, sequence, snapshot) = {
            let Some(compat) = &self.compat else { return };
            let Ok(mut state) = compat.lock() else { return };
            let Some(watch) = &state.x11_watch else {
                return;
            };
            let selected = watch.selection;
            let snapshot = Snapshot::capture(&state, selected.window);
            let Some(watch) = &mut state.x11_watch else {
                return;
            };
            if watch.sequence >= 64 || watch.last == Some(snapshot) {
                return;
            }
            watch.sequence += 1;
            watch.last = Some(snapshot);
            (selected, watch.sequence, snapshot)
        };
        // This separate read cannot grant authority or revise the ledger sample.
        // The fixture holds the opposite owner while using a state as a barrier.
        let writer = self.primary_writer_owner().ok();
        let value_position = writer.and_then(PrimaryWriter::observed_value_position);
        let value = Progress {
            phase: "progress",
            cut,
            sequence,
            window: selected.window.id.0,
            incarnation: selected.window.incarnation.0,
            main: selected.main,
            renderer: selected.renderer,
            main_native_id: selected.main_native_id,
            caller_allocation: selected.caller_allocation,
            control_ids: selected.control_ids,
            state: snapshot,
            value_position,
        };
        if let Ok(text) = serde_json::to_string(&value) {
            eprintln!("KELD_X11_NATIVE_PRESSURE {text}");
        }
    }
}
