//! Two representations of the same async source's existing receipt field.
//! Completed metadata/failure stays with that source until its ordinary service.
use super::failure::PrimaryWriteFailure;
use super::{RECORD_METADATA_BYTES, WriteReceipt, WriterRefusal};

pub(in crate::app_session) enum RetainedWrite {
    Pending(WriteReceipt),
    Completed(Option<PrimaryWriteFailure>),
}
impl RetainedWrite {
    pub(in crate::app_session) fn new(receipt: WriteReceipt) -> Self {
        Self::Pending(receipt)
    }

    pub(in crate::app_session) fn register_completion_interest(&self) -> Result<(), WriterRefusal> {
        match self {
            Self::Pending(receipt) => receipt.register_completion_interest(),
            Self::Completed(_) => Ok(()),
        }
    }

    /// Returns true only on the actual Pending -> Completed transition. Poll or
    /// evidence errors retain the unchanged Pending receipt and its exact custody.
    pub(in crate::app_session) fn settle(&mut self) -> Result<bool, WriterRefusal> {
        let Self::Pending(receipt) = self else {
            return Ok(false);
        };
        if receipt.poll()?.is_none() {
            receipt.register_completion_interest()?;
            return Ok(false);
        }
        let failure = receipt.failure_evidence()?;
        *self = Self::Completed(failure); // actual completion precedes receipt Drop
        Ok(true)
    }

    /// Ordinary source service alone consumes completed metadata/opaque failure.
    /// A pending physical receipt cannot be taken through this narrow operation.
    #[expect(
        clippy::option_option,
        reason = "Outer absence means no completed record; inner absence means an actually completed successful write."
    )]
    pub(in crate::app_session) fn take_completed(
        record: &mut Option<Self>,
    ) -> Option<Option<PrimaryWriteFailure>> {
        if !matches!(record, Some(Self::Completed(_))) {
            return None;
        }
        match record.take() {
            Some(Self::Completed(failure)) => Some(failure),
            _ => None,
        }
    }
}
const _: () = assert!(std::mem::size_of::<RetainedWrite>() <= RECORD_METADATA_BYTES);
