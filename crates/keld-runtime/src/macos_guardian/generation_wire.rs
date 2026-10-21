//! Private KGC1 generation-control record, codec and bounded incremental reader.
//! Guardian orchestration, admission and lifecycle decisions stay with the parent.

use std::io::{self, Read, Write};
use std::thread;
use std::time::Instant;

use super::{GUARDIAN_SHUTDOWN_DEADLINE, RevocationCause, RuntimeError, lifecycle_error};

const GENERATION_CONTROL_MAGIC: [u8; 4] = *b"KGC1";
const GENERATION_CONTROL_PAYLOAD_MAX: usize = 384;
pub(super) const GENERATION_CONTROL_RECORD_LEN: usize = 404;
pub(super) const CONTROL_PREPARE: u8 = 1;
pub(super) const CONTROL_PREPARED: u8 = 2;
pub(super) const CONTROL_SPAWNED: u8 = 3;
pub(super) const CONTROL_REGISTERED: u8 = 4;
pub(super) const CONTROL_REVOKE: u8 = 5;
pub(super) const CONTROL_REVOKED: u8 = 6;
pub(super) const CONTROL_CLEAR: u8 = 7;
pub(super) const CONTROL_CLEARED: u8 = 8;

#[derive(Debug)]
pub(super) struct GenerationControlRecord {
    pub(super) kind: u8,
    pub(super) cause: u8,
    pub(super) attempt: u32,
    pub(super) pid: u32,
    pub(super) payload: Vec<u8>,
}

impl GenerationControlRecord {
    pub(super) fn new(kind: u8, attempt: u32) -> Self {
        Self {
            kind,
            cause: 0,
            attempt,
            pid: 0,
            payload: Vec::new(),
        }
    }

    pub(super) fn with_pid(mut self, pid: u32) -> Self {
        self.pid = pid;
        self
    }

    pub(super) fn with_cause(mut self, cause: RevocationCause) -> Self {
        self.cause = encode_revocation_cause(cause);
        self
    }

    pub(super) fn with_payload(mut self, payload: impl Into<Vec<u8>>) -> Self {
        self.payload = payload.into();
        self
    }
}

fn is_control_poll_timeout(kind: io::ErrorKind) -> bool {
    matches!(kind, io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

pub(super) fn write_generation_control(
    writer: &mut impl Write,
    record: &GenerationControlRecord,
) -> Result<(), RuntimeError> {
    if record.payload.len() > GENERATION_CONTROL_PAYLOAD_MAX {
        return Err(lifecycle_error(
            "macOS guardian generation control encode",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "generation-control payload exceeds its fixed bound",
            ),
        ));
    }
    let payload_len = u16::try_from(record.payload.len()).map_err(|source| {
        lifecycle_error(
            "macOS guardian generation control encode",
            io::Error::new(io::ErrorKind::InvalidInput, source),
        )
    })?;
    let mut bytes = [0_u8; GENERATION_CONTROL_RECORD_LEN];
    bytes[..4].copy_from_slice(&GENERATION_CONTROL_MAGIC);
    bytes[4] = 1;
    bytes[5] = record.kind;
    bytes[6] = record.cause;
    bytes[8..12].copy_from_slice(&record.attempt.to_be_bytes());
    bytes[12..16].copy_from_slice(&record.pid.to_be_bytes());
    bytes[16..18].copy_from_slice(&payload_len.to_be_bytes());
    bytes[20..20 + record.payload.len()].copy_from_slice(&record.payload);
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut written = 0;
    while written < bytes.len() {
        match writer.write(&bytes[written..]) {
            Ok(0) => {
                return Err(lifecycle_error(
                    "macOS guardian generation control write",
                    io::Error::new(
                        io::ErrorKind::WriteZero,
                        "generation-control peer accepted zero bytes",
                    ),
                ));
            }
            Ok(count) => written += count,
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if is_control_poll_timeout(source.kind()) && Instant::now() < deadline => {
                thread::yield_now();
            }
            Err(source) => {
                return Err(lifecycle_error(
                    "macOS guardian generation control write",
                    source,
                ));
            }
        }
        if Instant::now() >= deadline && written < bytes.len() {
            return Err(lifecycle_error(
                "macOS guardian generation control write",
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "generation-control record exceeded its write deadline",
                ),
            ));
        }
    }
    writer
        .flush()
        .map_err(|source| lifecycle_error("macOS guardian generation control flush", source))
}

#[cfg(test)]
pub(super) fn read_generation_control(
    reader: &mut impl Read,
) -> Result<GenerationControlRecord, RuntimeError> {
    let mut bytes = [0_u8; GENERATION_CONTROL_RECORD_LEN];
    reader
        .read_exact(&mut bytes)
        .map_err(|source| lifecycle_error("macOS guardian generation control read", source))?;
    decode_generation_control(&bytes)
}

pub(super) struct GenerationControlReader {
    bytes: [u8; GENERATION_CONTROL_RECORD_LEN],
    pub(super) filled: usize,
    pub(super) started: Option<Instant>,
}

impl GenerationControlReader {
    pub(super) const fn new() -> Self {
        Self {
            bytes: [0; GENERATION_CONTROL_RECORD_LEN],
            filled: 0,
            started: None,
        }
    }

    pub(super) fn poll(
        &mut self,
        reader: &mut (impl Read + ?Sized),
    ) -> Result<Option<GenerationControlRecord>, RuntimeError> {
        self.poll_until(reader, None)
    }

    /// Only the initial authenticated Guardian record is constrained by the
    /// caller's original bootstrap deadline. Check inside the incremental
    /// read loop so a hostile byte-drip cannot avoid the outer host poll or
    /// extend admission by the separate per-record framing budget.
    pub(super) fn poll_until(
        &mut self,
        reader: &mut (impl Read + ?Sized),
        first_record_deadline: Option<Instant>,
    ) -> Result<Option<GenerationControlRecord>, RuntimeError> {
        loop {
            if first_record_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(initial_generation_record_timeout());
            }
            match reader.read(&mut self.bytes[self.filled..]) {
                Ok(0) => {
                    return Err(lifecycle_error(
                        "macOS guardian generation control read",
                        io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "generation-control peer closed mid-session",
                        ),
                    ));
                }
                Ok(read) => {
                    if first_record_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        return Err(initial_generation_record_timeout());
                    }
                    if self.filled == 0 {
                        self.started = Some(Instant::now());
                    }
                    self.filled += read;
                    // The final read must obey the same per-record deadline
                    // as every partial read. Completing the frame does not
                    // erase the retained start time or authorize a late ACK.
                    if self
                        .started
                        .is_some_and(|started| started.elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE)
                    {
                        return Err(lifecycle_error(
                            "macOS guardian generation control read",
                            io::Error::new(
                                io::ErrorKind::TimedOut,
                                "generation-control record exceeded its read deadline",
                            ),
                        ));
                    }
                    if self.filled == GENERATION_CONTROL_RECORD_LEN {
                        self.filled = 0;
                        self.started = None;
                        let record = decode_generation_control(&self.bytes)?;
                        self.bytes.fill(0);
                        return Ok(Some(record));
                    }
                }
                Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
                Err(source) if is_control_poll_timeout(source.kind()) => {
                    if self
                        .started
                        .is_some_and(|started| started.elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE)
                    {
                        return Err(lifecycle_error(
                            "macOS guardian generation control read",
                            io::Error::new(
                                io::ErrorKind::TimedOut,
                                "generation-control record exceeded its read deadline",
                            ),
                        ));
                    }
                    return Ok(None);
                }
                Err(source) => {
                    return Err(lifecycle_error(
                        "macOS guardian generation control read",
                        source,
                    ));
                }
            }
        }
    }
}

pub(super) fn initial_generation_record_timeout() -> RuntimeError {
    lifecycle_error(
        "macOS guardian initial generation registration deadline",
        io::Error::new(
            io::ErrorKind::TimedOut,
            "authenticated Guardian supplied no complete first KGC1 record before its original deadline",
        ),
    )
}

fn decode_generation_control(
    bytes: &[u8; GENERATION_CONTROL_RECORD_LEN],
) -> Result<GenerationControlRecord, RuntimeError> {
    if bytes[..4] != GENERATION_CONTROL_MAGIC
        || bytes[4] != 1
        || bytes[7] != 0
        || bytes[18] != 0
        || bytes[19] != 0
    {
        return Err(lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "generation-control header is malformed or unsupported",
            ),
        ));
    }
    let attempt = u32::from_be_bytes(bytes[8..12].try_into().map_err(|_| {
        lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(io::ErrorKind::InvalidData, "attempt field is truncated"),
        )
    })?);
    let pid = u32::from_be_bytes(bytes[12..16].try_into().map_err(|_| {
        lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(io::ErrorKind::InvalidData, "pid field is truncated"),
        )
    })?);
    let payload_len = usize::from(u16::from_be_bytes(bytes[16..18].try_into().map_err(
        |_| {
            lifecycle_error(
                "macOS guardian generation control decode",
                io::Error::new(io::ErrorKind::InvalidData, "payload length is truncated"),
            )
        },
    )?));
    if payload_len > GENERATION_CONTROL_PAYLOAD_MAX
        || bytes[20 + payload_len..].iter().any(|byte| *byte != 0)
    {
        return Err(lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "generation-control payload bound or padding is invalid",
            ),
        ));
    }
    Ok(GenerationControlRecord {
        kind: bytes[5],
        cause: bytes[6],
        attempt,
        pid,
        payload: bytes[20..20 + payload_len].to_vec(),
    })
}

pub(super) fn validate_control_response(
    request: &GenerationControlRecord,
    response: &GenerationControlRecord,
) -> Result<(), RuntimeError> {
    let valid = match response.kind {
        CONTROL_PREPARED => {
            request.kind == CONTROL_PREPARE
                && response.pid == 0
                && response.cause == 0
                && !response.payload.is_empty()
        }
        CONTROL_REGISTERED => {
            request.kind == CONTROL_SPAWNED
                && response.pid == request.pid
                && response.cause == 0
                && response.payload.is_empty()
        }
        CONTROL_REVOKED => {
            request.kind == CONTROL_REVOKE
                && response.pid == 0
                && response.cause == 0
                && response.payload.is_empty()
        }
        CONTROL_CLEARED => {
            request.kind == CONTROL_CLEAR
                && response.pid == request.pid
                && response.cause == 0
                && response.payload.is_empty()
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(lifecycle_error(
            "macOS guardian generation control transition",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "generation-control response fields do not match the requested transition",
            ),
        ))
    }
}

const fn encode_revocation_cause(cause: RevocationCause) -> u8 {
    match cause {
        RevocationCause::ChildExited => 1,
        RevocationCause::Shutdown => 2,
        RevocationCause::CaptureFailed => 3,
        RevocationCause::AdmissionFailed => 4,
        RevocationCause::SpawnFailed => 5,
        RevocationCause::WaitFailed => 6,
    }
}

pub(super) fn decode_revocation_cause(value: u8) -> Result<RevocationCause, RuntimeError> {
    match value {
        1 => Ok(RevocationCause::ChildExited),
        2 => Ok(RevocationCause::Shutdown),
        3 => Ok(RevocationCause::CaptureFailed),
        4 => Ok(RevocationCause::AdmissionFailed),
        5 => Ok(RevocationCause::SpawnFailed),
        6 => Ok(RevocationCause::WaitFailed),
        _ => Err(lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(io::ErrorKind::InvalidData, "unknown revocation cause"),
        )),
    }
}
