//! One synchronous source keeps this result until its publication turn ends.
//! Waiting observes only the original receipt; recovery stays with its real owner.

use super::{PrimaryWriteFailure, WriteCompletion, WriteReceipt, WriterLink};
use crate::app_session::{HostAppError, PrimaryRouterHandle, app_detail, append_app_cleanup};
use std::sync::Arc;
use std::time::Instant;

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
        let observed = receipt.wait(deadline);
        if observed == Ok(WriteCompletion::Written) {
            let result = receipt
                .written_at()
                .map_err(|error| error.into_host_error(phase))
                .and_then(|instant| {
                    instant.ok_or_else(|| {
                        app_detail(phase, "Written has no physical completion instant")
                    })
                });
            // A known Written outcome never becomes failed-writer authority,
            // even if reading its retained timestamp detects an internal error.
            drop(receipt);
            return DeferredPrimaryWrite {
                result,
                failure: None,
            };
        }
        let primary = observed
            .map_err(|error| error.into_host_error(phase))
            .and_then(|completion| completion.into_result(phase))
            .err()
            .unwrap_or_else(|| app_detail(phase, "unexpected physical completion state"));
        let evidence = receipt.failure_evidence();
        drop(receipt); // the worker still owns any actually pending encoded Box
        match evidence {
            Ok(failure) => {
                let retirement = failure.as_ref().map_or_else(
                    || self.retire_generation_after_write_failure(target.caller()),
                    |failure| self.record_primary_write_failure(failure),
                );
                DeferredPrimaryWrite {
                    result: Err(append_app_cleanup(primary, [retirement])),
                    failure,
                }
            }
            Err(error) => DeferredPrimaryWrite {
                // Poison/invalid observation supplies no fabricated Unknown fact.
                // Existing receipt/worker custody still owns the actual position.
                result: Err(append_app_cleanup(
                    primary,
                    [Err(error.into_host_error(phase))],
                )),
                failure: None,
            },
        }
    }
}
