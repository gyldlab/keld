//! A successor receives retained lifecycle facts before its reader starts.
//! This owner consumes actual write failure evidence inline, never recursively.

use std::sync::Arc;
use std::time::Instant;

use super::{ControlMessage, LifecycleEvent, WriterLink};
use crate::app_session::primary_writer::PrimaryWriterFailure;
use crate::app_session::{
    HostAppError, PrimaryGenerationFailure, PrimaryRouterHandle, append_app_cleanup,
};

impl PrimaryRouterHandle {
    #[expect(
        clippy::result_large_err,
        clippy::needless_pass_by_value,
        reason = "Retain the exact writer target through replay and return its complete existing failure custody without boxing."
    )]
    pub(in crate::app_session) fn replay_retained_lifecycle(
        &self,
        target: Arc<WriterLink>,
        ready: bool,
        last_window: bool,
        deadline: Instant,
    ) -> Result<(), PrimaryGenerationFailure> {
        for event in [
            ready.then_some(LifecycleEvent::Ready),
            last_window.then_some(LifecycleEvent::LastWindowClosed),
        ]
        .into_iter()
        .flatten()
        {
            let prepared = (|| {
                let frame = self.prepare_primary_control(
                    target.caller(),
                    Arc::clone(&target),
                    ControlMessage::LifecycleEvent(event),
                    deadline,
                    "retained lifecycle replay reservation",
                )?;
                self.admit_primary_frame(target.caller(), frame)
            })();
            let receipt = match prepared {
                Ok(Some(receipt)) => receipt,
                Ok(None) => return Ok(()),
                Err(primary) => return self.refuse_retained_lifecycle(&target, primary),
            };
            let observed = receipt
                .wait(deadline)
                .map_err(|error| error.into_host_error("retained lifecycle replay"))
                .and_then(|completion| completion.into_result("retained lifecycle replay"));
            if let Err(primary) = observed {
                let evidence = receipt
                    .failure_evidence()
                    .map_err(|error| error.into_host_error("retained replay failure evidence"));
                drop(receipt);
                match evidence {
                    Ok(Some(failure)) => {
                        let retired = self.record_primary_write_failure(&failure);
                        // The bound owner consumes this exact slot-derived fact
                        // inline; no numeric failure is restart authority.
                        return Err(PrimaryGenerationFailure {
                            writer_failure: Some(PrimaryWriterFailure::Write(failure)),
                            retired_after_failed_ready: None,
                            error: append_app_cleanup(primary, [retired]),
                        });
                    }
                    Ok(None) => return self.refuse_retained_lifecycle(&target, primary),
                    Err(error) => {
                        return self.refuse_retained_lifecycle(
                            &target,
                            append_app_cleanup(primary, [Err(error)]),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[expect(
        clippy::result_large_err,
        reason = "Return the complete exact-source writer failure without allocation or lost custody."
    )]
    fn refuse_retained_lifecycle(
        &self,
        target: &Arc<WriterLink>,
        primary: HostAppError,
    ) -> Result<(), PrimaryGenerationFailure> {
        match self.record_live_ready_refusal(target) {
            // Retirement/replacement won the same canonical cut. This source
            // has no failure authority over a successor, including equal numbers.
            Ok(None) => Ok(()),
            Ok(Some(refused)) => Err(PrimaryGenerationFailure {
                writer_failure: Some(PrimaryWriterFailure::Ready(refused.failure)),
                retired_after_failed_ready: None,
                error: append_app_cleanup(primary, [refused.cleanup]),
            }),
            Err(cleanup) => Err(PrimaryGenerationFailure {
                writer_failure: None,
                retired_after_failed_ready: None,
                error: append_app_cleanup(primary, [Err(cleanup)]),
            }),
        }
    }
}
