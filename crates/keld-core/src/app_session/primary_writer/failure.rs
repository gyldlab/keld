//! Failed-write facts originate only from this physical writer's exact slots.
//! They retain link metadata, never the encoded payload or a new recovery owner.

use std::sync::Weak;

use super::{Arc, Instant, Shared, WriteCompletion, WriteReceipt, WriterLink, WriterRefusal};

mod owner;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FailureKind {
    UnknownWrite,
    ReadyRefused,
}

/// An opaque fact from an admitted physical write, not an arbitrary caller flag.
/// Only the canonical generation owner may use it to decide recovery.
pub(in crate::app_session) struct PrimaryWriteFailure {
    target: Arc<WriterLink>,
    owner: Weak<Shared>,
}

/// A live initial Ready was refused under its exact current-generation fence.
/// Unlike `PrimaryWriteFailure`, this never claims an unknown physical effect.
pub(in crate::app_session) struct PrimaryReadyFailure {
    target: Arc<WriterLink>,
    owner: Weak<Shared>,
}

/// One bounded metadata record in the existing `SessionTransition`. Weak identities
/// retain neither a pipe handle nor payload/worker buffers after their owners end.
pub(in crate::app_session) struct RetiredWriterFailure {
    target: Weak<WriterLink>,
    owner: Weak<Shared>,
    kind: FailureKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app_session) enum FailureDisposition {
    Ignored,
    RestartRequested,
    RecoveryDenied,
}

#[derive(Debug)]
pub(in crate::app_session) enum PrimaryWriterFailure {
    Write(PrimaryWriteFailure),
    Ready(PrimaryReadyFailure),
}

impl std::fmt::Debug for PrimaryWriteFailure {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output
            .debug_struct("PrimaryWriteFailure")
            .field("attempt", &self.target.caller().attempt)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PrimaryReadyFailure {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output
            .debug_struct("PrimaryReadyFailure")
            .field("attempt", &self.target.caller().attempt)
            .finish_non_exhaustive()
    }
}

impl PrimaryWriteFailure {
    /// Called by the sole worker only after the actual native write returned
    /// `OutcomeUnknown`, before that exact slot publishes its completion.
    pub(super) fn returned_unknown(shared: &Arc<Shared>, target: Arc<WriterLink>) -> Self {
        Self {
            target,
            owner: Arc::downgrade(shared),
        }
    }
}

pub(super) enum WriteReceiptState {
    Pending,
    Written(Instant),
    OutcomeUnknown(PrimaryWriteFailure),
    AdmittedDeadlineExpired(PrimaryWriteFailure),
}

impl WriteReceipt {
    /// Read completion, its physical time and failure authority in one exact
    /// slot observation. An earlier observer cutoff has no writer authority.
    pub(super) fn snapshot(
        &self,
        expected: Option<&Arc<WriterLink>>,
    ) -> Result<WriteReceiptState, WriterRefusal> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?;
        let completion = state.completion(self.position)?;
        let target = state.slots[self.position.index].target.as_ref();
        if expected
            .is_some_and(|expected| target.is_none_or(|target| !Arc::ptr_eq(expected, target)))
        {
            return Err(WriterRefusal::InvalidFrame);
        }
        let evidence = || {
            let target = target.ok_or(WriterRefusal::InvalidFrame)?;
            Ok(PrimaryWriteFailure {
                target: Arc::clone(target),
                owner: Arc::downgrade(&self.shared),
            })
        };
        match completion {
            Some(WriteCompletion::Written) => state
                .written_at(self.position)?
                .map(WriteReceiptState::Written)
                .ok_or(WriterRefusal::InvalidFrame),
            Some(WriteCompletion::OutcomeUnknown) => {
                evidence().map(WriteReceiptState::OutcomeUnknown)
            }
            None if state.pending(self.position) && Instant::now() >= self.deadline => {
                evidence().map(WriteReceiptState::AdmittedDeadlineExpired)
            }
            None => Ok(WriteReceiptState::Pending),
        }
    }

    /// Inspect before dropping the unique receipt. A successful write, a mere
    /// wake, a short observer timeout, or an unadmitted frame yields no evidence.
    pub(in crate::app_session) fn failure_evidence(
        &self,
    ) -> Result<Option<PrimaryWriteFailure>, WriterRefusal> {
        match self.snapshot(None)? {
            WriteReceiptState::OutcomeUnknown(failure)
            | WriteReceiptState::AdmittedDeadlineExpired(failure) => Ok(Some(failure)),
            WriteReceiptState::Pending | WriteReceiptState::Written(_) => Ok(None),
        }
    }
}

impl PrimaryWriterFailure {
    fn parts(&self) -> (&Arc<WriterLink>, &Weak<Shared>, FailureKind) {
        match self {
            Self::Write(failure) => (&failure.target, &failure.owner, FailureKind::UnknownWrite),
            Self::Ready(failure) => (&failure.target, &failure.owner, FailureKind::ReadyRefused),
        }
    }
}

impl RetiredWriterFailure {
    fn matches(&self, target: &Arc<WriterLink>, owner: &Weak<Shared>, kind: FailureKind) -> bool {
        self.kind == kind
            && Weak::ptr_eq(&self.target, &Arc::downgrade(target))
            && Weak::ptr_eq(&self.owner, owner)
    }
}
