//! Finite scheduling of one actual worker-owned VALUE packet for X10.
//! No payload copy, new queue, native call, admission or release authority.
#![forbid(unsafe_code)]

use super::{HostAppError, Packet, Position, PrimaryWriter, Shared, WriteCompletion, app_detail};
use keld_ipc::{CorrelationId, FrameHeader, FrameKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RECHECK: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(in crate::app_session) struct ValuePosition {
    pub(super) serial: u64,
    pub(super) phase: &'static str,
    pub(super) target_id: Option<usize>,
    pub(super) packet_in_slot: bool,
    pub(super) receipt_live: bool,
}

impl PrimaryWriter {
    /// Passive existing-slot scalars; no reservation, payload or receipt is copied.
    pub(in crate::app_session) fn observed_value_position(&self) -> Option<ValuePosition> {
        self.shared
            .state
            .lock()
            .ok()
            .map(|state| state.observed_value_position())
    }
}

pub(super) struct PhysicalValueHold {
    directory: PathBuf,
    selected: bool,
}

#[derive(Clone, Copy, serde::Serialize)]
pub(super) struct HeldPacket {
    index: usize,
    serial: u64,
    kind: u8,
    compat_control: bool,
    correlation: u32,
    payload_id: usize,
    payload_bytes: usize,
    target_id: usize,
    reader_stop_id: usize,
}

fn failure(detail: &'static str) -> HostAppError {
    app_detail("X10 physical VALUE hold", detail)
}

fn control_file(path: &Path) -> Result<bool, HostAppError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(failure("control marker is not an ordinary file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(failure("control marker cannot be inspected")),
    }
}

fn candidate(position: Position, header: &FrameHeader) -> bool {
    position.index == 0
        && header.kind == FrameKind::Event
        && header.channel == keld_ipc::channel_table::COMPAT_CONTROL.id()
        && header.corr == CorrelationId(0)
}

impl PhysicalValueHold {
    /// Captured once by the existing worker owner before starting its thread.
    pub(super) fn from_env() -> Result<Option<Self>, HostAppError> {
        let Some(value) = std::env::var_os(super::TEST_PRIMARY_VALUE_HOLD_ENV) else {
            return Ok(None);
        };
        let directory = PathBuf::from(value);
        if !directory.is_absolute() || directory.as_os_str().len() > 4096 {
            return Err(failure("expected one bounded absolute control directory"));
        }
        let metadata = std::fs::symlink_metadata(&directory)
            .map_err(|_| failure("control directory cannot be inspected"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(failure("control directory is not an ordinary directory"));
        }
        for marker in ["arm", "release"] {
            if control_file(&directory.join(marker))? {
                return Err(failure("control directory must start without markers"));
            }
        }
        Ok(Some(Self {
            directory,
            selected: false,
        }))
    }

    fn released(&self, deadline: Instant, now: Instant) -> Result<bool, HostAppError> {
        // The real Packet's original deadline wins even if release is already present.
        if now >= deadline {
            return Err(failure("original physical Packet deadline expired"));
        }
        if !control_file(&self.directory.join("arm"))? {
            return Err(failure("the same armed physical hold disappeared"));
        }
        control_file(&self.directory.join("release"))
    }

    /// The same Packet remains borrowed in run, outside state/Core/stream guards.
    /// A false condition only parks this existing worker until its next file check.
    pub(super) fn hold(
        &mut self,
        position: Position,
        packet: &Packet,
        shared: &Shared,
    ) -> Result<Option<HeldPacket>, HostAppError> {
        if self.selected || !candidate(position, &packet.header) {
            return Ok(None);
        }
        if control_file(&self.directory.join("release"))? {
            return Err(failure(
                "release appeared before a real physical Packet was held",
            ));
        }
        if !control_file(&self.directory.join("arm"))? {
            return Ok(None);
        }
        self.selected = true;
        let held = HeldPacket::capture(position, packet);
        held.observe("packet-held", shared, None, Some(packet));
        loop {
            match self.released(packet.deadline, Instant::now()) {
                Ok(true) => {
                    held.observe("packet-released", shared, None, Some(packet));
                    return Ok(Some(held));
                }
                Ok(false) => std::thread::park_timeout(
                    RECHECK.min(packet.deadline.saturating_duration_since(Instant::now())),
                ),
                Err(error) => {
                    held.observe("packet-hold-failed", shared, None, Some(packet));
                    return Err(error);
                }
            }
        }
    }
}

impl HeldPacket {
    fn capture(position: Position, packet: &Packet) -> Self {
        Self {
            index: position.index,
            serial: position.serial,
            kind: packet.header.kind as u8,
            compat_control: packet.header.channel == keld_ipc::channel_table::COMPAT_CONTROL.id(),
            correlation: packet.header.corr.0,
            payload_id: packet.payload.as_ptr() as usize,
            payload_bytes: packet.payload.len(),
            target_id: Arc::as_ptr(&packet.target) as usize,
            reader_stop_id: Arc::as_ptr(&packet.target.caller().reader_stop) as usize,
        }
    }
    /// Call at actual write return and after the original Packet Drop, respectively.
    pub(super) fn observe(
        self,
        phase: &'static str,
        shared: &Shared,
        outcome: Option<WriteCompletion>,
        packet: Option<&Packet>,
    ) {
        let value = shared
            .state
            .lock()
            .ok()
            .map(|state| state.observed_value_position());
        let outcome = outcome.map(|value| match value {
            WriteCompletion::Written => "written",
            WriteCompletion::OutcomeUnknown => "outcome-unknown",
        });
        eprintln!(
            "KELD_X10_PHYSICAL_VALUE {}",
            serde_json::json!({
                "phase": phase, "packet": self, "actual_packet": packet.map(|packet| Self::capture(Position { index: self.index, serial: self.serial }, packet)),
                "value_position": value, "outcome": outcome,
            })
        );
    }
}

#[cfg(test)]
mod tests;
