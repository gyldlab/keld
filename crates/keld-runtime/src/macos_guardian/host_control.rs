//! Host-side KGC1 request/response exchange and exact cancellation attribution.
//! The wire codec and guardian lifecycle stay in their existing owners.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::{
    CONTROL_PREPARE, GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlReader, GenerationControlRecord,
    RuntimeError, lifecycle_error, validate_control_response, write_generation_control,
};

pub(super) struct GenerationControlPeer {
    pub(super) stream: std::os::unix::net::UnixStream,
    pub(super) reader: GenerationControlReader,
}

pub(super) fn control_exchange(
    control: &Arc<Mutex<GenerationControlPeer>>,
    request: &GenerationControlRecord,
    expected_kind: u8,
    expected_attempt: u32,
) -> Result<GenerationControlRecord, RuntimeError> {
    control_exchange_with_cancellation(control, request, expected_kind, expected_attempt, None)
}

pub(super) fn control_exchange_with_cancellation(
    control: &Arc<Mutex<GenerationControlPeer>>,
    request: &GenerationControlRecord,
    expected_kind: u8,
    expected_attempt: u32,
    cancellation: Option<(&AtomicBool, &AtomicBool)>,
) -> Result<GenerationControlRecord, RuntimeError> {
    let mut control = match control.lock() {
        Ok(control) => control,
        Err(poisoned) => poisoned.into_inner(),
    };
    let GenerationControlPeer { stream, reader } = &mut *control;
    write_generation_control(stream, request)?;
    let native_wait = request.kind == CONTROL_PREPARE && request.attempt > 1;
    let deadline = (!native_wait).then(|| Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE);
    let response = loop {
        if native_wait
            && reader.filled == 0
            && let Some((cancelled, host_unavailable)) = cancellation
        {
            if cancelled.load(Ordering::Acquire) {
                return Err(interrupted_successor_preparation(expected_attempt));
            }
            if host_unavailable.load(Ordering::Acquire) {
                return Err(lifecycle_error(
                    "macOS guardian generation control transition",
                    io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "host-liveness observation failed",
                    ),
                ));
            }
        }
        if let Some(response) = reader.poll(stream)? {
            break response;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(lifecycle_error(
                "macOS guardian generation control transition",
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "host did not acknowledge the generation transition before the deadline",
                ),
            ));
        }
    };
    if response.kind != expected_kind || response.attempt != expected_attempt {
        return Err(lifecycle_error(
            "macOS guardian generation control transition",
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "expected kind {expected_kind} attempt {expected_attempt}, received kind {} attempt {}",
                    response.kind, response.attempt
                ),
            ),
        ));
    }
    validate_control_response(request, &response)?;
    Ok(response)
}

#[derive(Debug)]
struct OwnedPreparationInterruption {
    attempt: u32,
}

impl std::fmt::Display for OwnedPreparationInterruption {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "owned shutdown interrupted successor attempt {}",
            self.attempt
        )
    }
}

impl std::error::Error for OwnedPreparationInterruption {}

pub(super) fn interrupted_successor_preparation(attempt: u32) -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation control transition",
        io::Error::new(
            io::ErrorKind::Interrupted,
            OwnedPreparationInterruption { attempt },
        ),
    )
}

/// Recognizes only this preparer's private interruption for the exact attempt.
pub(crate) fn is_owned_preparation_interruption(error: &RuntimeError, attempt: u32) -> bool {
    matches!(error, RuntimeError::Lifecycle { source, .. }
        if source.get_ref()
            .and_then(|source| source.downcast_ref::<OwnedPreparationInterruption>())
            .is_some_and(|marker| marker.attempt == attempt && attempt > 1))
}
