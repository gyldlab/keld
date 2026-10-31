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
    let response = await_control_response(
        stream,
        reader,
        expected_attempt,
        native_wait,
        deadline,
        cancellation,
        Instant::now,
    )?;
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

/// The one native generation response wait, including the successor's explicit
/// host-shutdown cancellation contract. The injected monotonic clock is the
/// real `Instant::now` in production and lets a native test prove the read-end
/// deadline without using timing sleeps or changing shipping limits.
fn await_control_response(
    stream: &mut std::os::unix::net::UnixStream,
    reader: &mut GenerationControlReader,
    expected_attempt: u32,
    native_wait: bool,
    deadline: Option<Instant>,
    cancellation: Option<(&AtomicBool, &AtomicBool)>,
    mut now: impl FnMut() -> Instant,
) -> Result<GenerationControlRecord, RuntimeError> {
    loop {
        // For bounded transitions an already-expired full reply is no more
        // authoritative than an expired partial frame. Successor PREPARE has
        // an explicit unbounded-until-cancel contract and no deadline here.
        if deadline.is_some_and(|deadline| now() >= deadline) {
            return Err(control_response_timeout());
        }
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
            // A native poll may return a full valid frame after the deadline
            // even when the clock was still valid before its final read.
            if deadline.is_some_and(|deadline| now() >= deadline) {
                return Err(control_response_timeout());
            }
            return Ok(response);
        }
        if deadline.is_some_and(|deadline| now() >= deadline) {
            return Err(control_response_timeout());
        }
    }
}

fn control_response_timeout() -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation control transition",
        io::Error::new(
            io::ErrorKind::TimedOut,
            "host did not acknowledge the generation transition before the deadline",
        ),
    )
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

#[cfg(test)]
mod tests {
    use super::{GenerationControlPeer, await_control_response, control_exchange};
    use crate::macos_guardian::{
        CONTROL_CLEARED, CONTROL_REGISTERED, CONTROL_REVOKE, CONTROL_REVOKED,
        GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlReader, GenerationControlRecord, RuntimeError,
        write_generation_control,
    };
    use std::io::{self, Write as _};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn exchange_with_buffered_native_response(
        response: &GenerationControlRecord,
        expected_kind: u8,
        expected_attempt: u32,
    ) -> Result<GenerationControlRecord, RuntimeError> {
        let (client, mut guardian) = UnixStream::pair().expect("private native KGC1 exchange");
        client
            .set_nonblocking(true)
            .expect("bounded native request/response poll");
        write_generation_control(&mut guardian, response)
            .expect("independently buffered real guardian response");
        let control = Arc::new(Mutex::new(GenerationControlPeer {
            stream: client,
            reader: GenerationControlReader::new(),
        }));
        control_exchange(
            &control,
            &GenerationControlRecord::new(CONTROL_REVOKE, 7),
            expected_kind,
            expected_attempt,
        )
    }

    #[test]
    fn expired_complete_kgc1_response_cannot_authorize_generation() {
        let (mut client, mut host) = UnixStream::pair().expect("one private KGC1 pair");
        client
            .set_nonblocking(true)
            .expect("normal responsive control stream");
        let record = GenerationControlRecord::new(CONTROL_REGISTERED, 7).with_pid(791);
        let mut encoded = Vec::new();
        write_generation_control(&mut encoded, &record)
            .expect("native fixed-size authentic response frame");
        host.write_all(&encoded)
            .expect("complete buffered response");
        let expired = Instant::now()
            .checked_sub(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("a retained expired monotonic deadline");
        let refusal = await_control_response(
            &mut client,
            &mut GenerationControlReader::new(),
            7,
            false,
            Some(expired),
            None,
            Instant::now,
        )
        .expect_err("buffered complete KGC1 cannot authorize an expired transition");
        assert!(
            matches!(refusal, RuntimeError::Lifecycle { source, .. }
                if source.kind() == io::ErrorKind::TimedOut),
            "retained deadline must surface a typed transition timeout"
        );
    }

    #[test]
    fn timely_native_kgc1_response_preserves_original_record_bytes() {
        let (mut client, mut host) = UnixStream::pair().expect("one private KGC1 pair");
        client
            .set_nonblocking(true)
            .expect("normal responsive KGC1 stream");
        let record = GenerationControlRecord::new(CONTROL_REGISTERED, 7).with_pid(791);
        let mut encoded = Vec::new();
        write_generation_control(&mut encoded, &record).expect("encode real control frame");
        host.write_all(&encoded)
            .expect("complete authentic response");
        let received = await_control_response(
            &mut client,
            &mut GenerationControlReader::new(),
            7,
            false,
            Some(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE),
            None,
            Instant::now,
        )
        .expect("valid response before original deadline");
        assert_eq!(received.kind, record.kind);
        assert_eq!(received.attempt, record.attempt);
        assert_eq!(received.pid, record.pid);
        assert_eq!(received.cause, record.cause);
        assert_eq!(received.payload, record.payload);
    }

    #[test]
    fn complete_kgc1_response_finishing_at_deadline_cannot_authorize_generation() {
        let (mut client, mut guardian) = UnixStream::pair().expect("real native KGC1 pair");
        client
            .set_nonblocking(true)
            .expect("bounded native KGC1 response polling");
        let record = GenerationControlRecord::new(CONTROL_REGISTERED, 7).with_pid(791);
        write_generation_control(&mut guardian, &record)
            .expect("full response available to the native reader");
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        let mut clock_checks = 0;
        let refusal = await_control_response(
            &mut client,
            &mut GenerationControlReader::new(),
            7,
            false,
            Some(deadline),
            None,
            || {
                clock_checks += 1;
                if clock_checks == 1 {
                    deadline
                        .checked_sub(Duration::from_millis(1))
                        .expect("a future native test deadline permits one preceding tick")
                } else {
                    deadline
                }
            },
        )
        .expect_err("a reply completing at the deadline cannot authorize the generation");
        assert_eq!(
            clock_checks, 2,
            "native poll must cross the injected deadline"
        );
        assert!(matches!(refusal, RuntimeError::Lifecycle { source, .. }
            if source.kind() == io::ErrorKind::TimedOut));
    }

    #[test]
    fn native_kgc1_exchange_rejects_wrong_kind_attempt_and_fields() {
        let timely = exchange_with_buffered_native_response(
            &GenerationControlRecord::new(CONTROL_REVOKED, 7),
            CONTROL_REVOKED,
            7,
        )
        .expect("valid native transition retains caller validation");
        assert_eq!(timely.kind, CONTROL_REVOKED);
        assert_eq!(timely.attempt, 7);

        // Kind is independently expected by the caller. The response would
        // otherwise satisfy the REVOKE validator, so the outer check matters.
        let wrong_kind = exchange_with_buffered_native_response(
            &GenerationControlRecord::new(CONTROL_REVOKED, 7),
            CONTROL_CLEARED,
            7,
        )
        .expect_err("wrong caller-expected kind cannot authorize a transition");
        assert!(matches!(wrong_kind, RuntimeError::Lifecycle { source, .. }
            if source.kind() == io::ErrorKind::InvalidData));

        // Response field validation does not validate the generation attempt.
        let wrong_attempt = exchange_with_buffered_native_response(
            &GenerationControlRecord::new(CONTROL_REVOKED, 8),
            CONTROL_REVOKED,
            7,
        )
        .expect_err("wrong generation attempt cannot authorize a transition");
        assert!(
            matches!(wrong_attempt, RuntimeError::Lifecycle { source, .. }
            if source.kind() == io::ErrorKind::InvalidData)
        );

        let wrong_fields = exchange_with_buffered_native_response(
            &GenerationControlRecord::new(CONTROL_REVOKED, 7).with_pid(791),
            CONTROL_REVOKED,
            7,
        )
        .expect_err("unexpected PID cannot mint a valid revoke receipt");
        assert!(
            matches!(wrong_fields, RuntimeError::Lifecycle { source, .. }
            if source.kind() == io::ErrorKind::InvalidData)
        );
    }
}
