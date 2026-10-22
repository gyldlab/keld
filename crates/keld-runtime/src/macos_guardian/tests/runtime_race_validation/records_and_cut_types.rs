//! Named test records and fixed-width relay receipt types for KEL-143 races.
use std::io;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use super::{
    ChildPreparer, GENERATION_CONTROL_POLL, GenerationControlReader, GenerationControlRecord,
    GuardedPrimaryLease, GuardedPrimaryPreparer, PreparedChild, RuntimeError, lifecycle_error,
};

pub(super) const COLLECTION: Duration = Duration::from_secs(7);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeliveryCut {
    Zero,
    Partial,
    FullBeforeLease,
    LeaseBeforeSpawn,
}

pub(super) enum RelayCommand {
    Full,
    Prefix,
    AwaitClosure,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ControlFields {
    pub(super) kind: u8,
    pub(super) cause: u8,
    pub(super) attempt: u32,
    pub(super) pid: u32,
    pub(super) payload_len: usize,
}

impl ControlFields {
    pub(super) fn capture(record: &GenerationControlRecord) -> Self {
        Self {
            kind: record.kind,
            cause: record.cause,
            attempt: record.attempt,
            pid: record.pid,
            payload_len: record.payload.len(),
        }
    }
}

pub(super) enum RelayReceipt {
    ClearedOne,
    PreparedTwo(GenerationControlRecord),
    PrefixSent(Instant),
    NormalRevokeTwo {
        request: ControlFields,
        response: ControlFields,
    },
    NormalClearTwo {
        request: ControlFields,
        response: ControlFields,
    },
    GuardianControlEof,
}

pub(super) struct PhaseGate {
    pub(super) entered: Sender<()>,
    pub(super) release: Receiver<()>,
}

impl PhaseGate {
    pub(super) fn wait(&self) -> Result<(), RuntimeError> {
        self.entered.send(()).map_err(|_| {
            lifecycle_error(
                "test Runtime phase receipt",
                io::Error::other("custodian ended"),
            )
        })?;
        self.release.recv_timeout(COLLECTION).map_err(|source| {
            lifecycle_error("test Runtime phase release", io::Error::other(source))
        })
    }
}

pub(super) struct ReturnedLeasePreparer<C> {
    pub(super) inner: GuardedPrimaryPreparer<C>,
    pub(super) gate: Option<PhaseGate>,
}

impl<C> ChildPreparer for ReturnedLeasePreparer<C>
where
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
{
    type Lease = GuardedPrimaryLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let prepared = self.inner.prepare(attempt)?;
        // This receipt follows actual GuardedPrimaryLease construction.
        // It does not replace that lease or call another spawn owner.
        if attempt == 2
            && let Some(gate) = &self.gate
        {
            gate.wait()?;
        }
        Ok(prepared)
    }
}

pub(super) fn timed_pair() -> (UnixStream, UnixStream) {
    let (one, two) = UnixStream::pair().expect("existing private Unix pair");
    for stream in [&one, &two] {
        stream
            .set_read_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("read poll");
        stream
            .set_write_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("write poll");
    }
    (one, two)
}

pub(super) fn receive_record(
    stream: &mut UnixStream,
) -> Result<GenerationControlRecord, RuntimeError> {
    receive_record_until(stream, Instant::now() + COLLECTION, |_| {})
}

#[derive(Clone, Copy, Debug)]
enum NativeRecordStage {
    BeforePoll,
    AfterCompleteRecord,
}

// Private fixture hook, not an event or production authority. The caller's
// original clock spans both the actual kernel read and complete-record return.
fn receive_record_until(
    stream: &mut UnixStream,
    deadline: Instant,
    mut observe: impl FnMut(NativeRecordStage),
) -> Result<GenerationControlRecord, RuntimeError> {
    let mut reader = GenerationControlReader::new();
    loop {
        if Instant::now() >= deadline {
            return Err(record_collection_timeout());
        }
        observe(NativeRecordStage::BeforePoll);
        if let Some(record) = reader.poll(stream)? {
            observe(NativeRecordStage::AfterCompleteRecord);
            if Instant::now() >= deadline {
                return Err(record_collection_timeout());
            }
            return Ok(record);
        }
        if Instant::now() >= deadline {
            return Err(record_collection_timeout());
        }
    }
}

fn record_collection_timeout() -> RuntimeError {
    lifecycle_error(
        "test Runtime record collection",
        io::Error::from(io::ErrorKind::TimedOut),
    )
}

#[cfg(test)]
#[path = "records_and_cut_types/deadline_tests.rs"]
mod deadline_tests;
