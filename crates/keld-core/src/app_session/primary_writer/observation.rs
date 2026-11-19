//! One synchronous source keeps this result until its publication turn ends.
//! Waiting observes only the original receipt; recovery stays with its real owner.

use super::failure::WriteReceiptState;
use super::{PrimaryWriteFailure, WriteCompletion, WriteReceipt, WriterLink, WriterRefusal};
use crate::app_session::{HostAppError, PrimaryRouterHandle, app_detail, append_app_cleanup};
use std::sync::Arc;
use std::time::Instant;

#[cfg(all(test, not(keld_core_native_renderer_main)))]
mod tests;

/// Actual completion and optional sealed failure, with no payload or new queue.
#[must_use = "report the retained write failure after releasing source publication guards"]
pub(in crate::app_session) struct DeferredPrimaryWrite {
    result: Result<Instant, HostAppError>,
    failure: Option<PrimaryWriteFailure>,
}
impl DeferredPrimaryWrite {
    /// The source drops its non-clone publication turn before reporting failure.
    /// Written carries the original physical instant, never the observer clock.
    pub(in crate::app_session) fn into_parts(
        self,
    ) -> (Result<Instant, HostAppError>, Option<PrimaryWriteFailure>) {
        (self.result, self.failure)
    }
}

impl PrimaryRouterHandle {
    /// No canonical/source lock may surround this wait. The source may retain its
    /// publication turn, because this method never submits or waits on the
    /// `DirectPrimaryOwner`. Only actual exact non-writing failure custody is cut.
    pub(in crate::app_session) fn observe_primary_write(
        &self,
        target: &Arc<WriterLink>,
        receipt: WriteReceipt,
        deadline: Instant,
        phase: &'static str,
    ) -> DeferredPrimaryWrite {
        let observed = receipt.wait(deadline.min(receipt.deadline));
        self.finish_primary_write_observation(target, receipt, observed, deadline, phase)
    }

    fn finish_primary_write_observation(
        &self,
        target: &Arc<WriterLink>,
        receipt: WriteReceipt,
        observed: Result<WriteCompletion, WriterRefusal>,
        observer_deadline: Instant,
        phase: &'static str,
    ) -> DeferredPrimaryWrite {
        let admitted_deadline = receipt.deadline;
        let snapshot = receipt.snapshot(Some(target));
        drop(receipt); // the worker still owns any actually pending encoded Box
        let wait_error = || match observed {
            Err(WriterRefusal::Deadline) if observer_deadline < admitted_deadline => app_detail(
                phase,
                "the observer cutoff expired before the admitted primary write cutoff",
            ),
            _ => observed
                .map_err(|error| error.into_host_error(phase))
                .and_then(|completion| completion.into_result(phase))
                .err()
                .unwrap_or_else(|| app_detail(phase, "unexpected physical completion state")),
        };
        match snapshot {
            Ok(WriteReceiptState::Written(at)) => DeferredPrimaryWrite {
                result: Ok(at),
                failure: None,
            },
            Ok(WriteReceiptState::Pending) => DeferredPrimaryWrite {
                result: Err(wait_error()),
                failure: None,
            },
            Ok(
                WriteReceiptState::OutcomeUnknown(failure)
                | WriteReceiptState::AdmittedDeadlineExpired(failure),
            ) => {
                let retirement = self.record_primary_write_failure(&failure);
                DeferredPrimaryWrite {
                    result: Err(append_app_cleanup(wait_error(), [retirement])),
                    failure: Some(failure),
                }
            }
            Err(error) => DeferredPrimaryWrite {
                // Poison/invalid observation supplies no fabricated Unknown fact.
                // Existing receipt/worker custody still owns the actual position.
                result: Err(append_app_cleanup(
                    wait_error(),
                    [Err(error.into_host_error(phase))],
                )),
                failure: None,
            },
        }
    }
}
