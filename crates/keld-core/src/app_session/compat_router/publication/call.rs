//! The one authenticated reader owns its decoded CALL and any immediate reply.
//! Native DATA admission and physical primary output retain their original records.
use super::{
    Arc, CallError, ControlMessage, CorrelationId, HostAppError, Instant, MAX_CONTROL_BYTES,
    PrimaryCallerContext, PrimaryRouterHandle, PublicationResult, WriterLink, admit_locked,
    app_detail, app_ipc, compat_control, refused, with_primary_caller_locked,
};
use crate::app_session::primary_writer::{PreparedFrame, ValueFreeTerminal};
use crate::app_session::window_router::PublicationError;
use keld_guard::DenyReason;

pub(super) enum CompatCallAnswer {
    Released,
    Stale,
    Capacity,
    Unsubscribed,
    Denied(DenyReason),
}
pub(super) enum CompatCallAdmission {
    Deferred,
    Immediate(CompatCallAnswer),
}

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "The original CALL keeps one admission, physical receipt and release sequence without splitting custody."
    )]
    pub(in crate::app_session) fn dispatch_compat(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: Vec<u8>,
    ) -> Result<(), HostAppError> {
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_compat_event_state("call.enter", Some((correlation.0, "entered")));
        self.primary_writer_owner()?;
        let Some(compat) = &self.compat else {
            return Err(app_detail("compat call", "compat service unavailable"));
        };
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let mut owns_control = false;
        let result = (|| -> PublicationResult<()> {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            let request = compat_control::decode_request(&payload)
                .map_err(|source| app_ipc("compat request", &source))?;
            drop(payload); // bounded canonical body is gone before effect admission
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            self.observe_compat_event_state(
                "call.decoded",
                Some((
                    correlation.0,
                    match &request {
                        compat_control::CompatControlRequest::DeliverToCurrentMain { .. } => {
                            "deliver-current"
                        }
                        compat_control::CompatControlRequest::DeliverToOriginalDocument {
                            ..
                        } => "deliver-original",
                        compat_control::CompatControlRequest::ReleaseConsumed { .. } => {
                            "release-consumed"
                        }
                    },
                )),
            );
            let mut entered = false;
            let mut early = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
                entered = true;
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("compat CALL admission", "ledger lock poisoned"))?;
                if !state.subscribed_to(caller) {
                    early = Some(CompatCallAnswer::Unsubscribed);
                    return Ok(());
                }
                if state
                    .ledger
                    .reserve_control(caller, correlation.0, MAX_CONTROL_BYTES)
                    .is_err()
                {
                    early = Some(CompatCallAnswer::Capacity);
                } else {
                    owns_control = true;
                }
                Ok(())
            })?;
            if !entered {
                return Ok(());
            }
            let answer = if let Some(answer) = early {
                Some(answer)
            } else {
                match self.admit_compat_request_owned(compat, caller, correlation, request)? {
                    Some(CompatCallAdmission::Immediate(answer)) => Some(answer),
                    Some(CompatCallAdmission::Deferred) | None => None,
                }
            };
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            self.observe_compat_event_state(
                "call.admission",
                Some((
                    correlation.0,
                    match &answer {
                        None => "no-immediate-answer",
                        Some(CompatCallAnswer::Released) => "released",
                        Some(CompatCallAnswer::Stale) => "stale",
                        Some(CompatCallAnswer::Capacity) => "capacity",
                        Some(CompatCallAnswer::Unsubscribed) => "unsubscribed",
                        Some(CompatCallAnswer::Denied(_)) => "denied",
                    },
                )),
            );
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            self.observe_queued_close_event_admission(correlation);
            let Some(answer) = answer else {
                return Ok(());
            };
            let mut frame = Some(self.prepare_compat_call_answer(
                caller,
                &target,
                correlation,
                answer,
                deadline,
            )?);
            let mut receipt = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
                let active = current.as_ref().ok_or_else(|| {
                    app_detail("compat reply admission", "validated caller disappeared")
                })?;
                let prepared = frame.take().ok_or_else(|| {
                    app_detail("compat reply admission", "prepared frame disappeared")
                })?;
                if owns_control {
                    let mut state = compat.lock().map_err(|_| {
                        app_detail("compat reply admission", "ledger lock poisoned")
                    })?;
                    let charge = state
                        .ledger
                        .controls
                        .iter_mut()
                        .flatten()
                        .find(|charge| {
                            charge.owned_by(caller)
                                && charge.correlation == correlation.0
                                && !charge.application_done
                                && charge.primary_write.is_none()
                        })
                        .ok_or_else(|| {
                            app_detail(
                                "compat reply admission",
                                "original correlated CONTROL record disappeared",
                            )
                        })?;
                    let written = admit_locked(active, prepared)?;
                    charge.primary_write = Some(written.observation()); // same record, immediate infallible commit
                    receipt = Some(written);
                } else {
                    receipt = Some(admit_locked(active, prepared)?);
                }
                Ok(())
            })?;
            drop(frame);
            if let Some(receipt) = receipt {
                // The unique source receipt survives through actual observation.
                // The correlation retains only its exact physical slot/serial.
                let interest = receipt
                    .register_completion_interest()
                    .map_err(|error| error.into_host_error("compat reply completion interest"));
                let observed =
                    self.observe_publication(&target, receipt, deadline, "compat reply completion");
                match observed {
                    Ok(()) => interest?,
                    Err(error) => return Err(error.with_cleanup(interest)),
                }
            }
            Ok(())
        })();
        let released = if owns_control {
            compat
                .lock()
                .map_err(|_| app_detail("compat source release", "ledger lock poisoned"))
                .map(|mut state| state.ledger.release_control(caller, correlation.0))
        } else {
            Ok(())
        };
        let result = match result {
            Ok(()) => released.map_err(PublicationError::from),
            Err(error) => Err(error.with_cleanup(released)),
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let result = result.and_then(|()| {
            self.x11_pressure_after_event_release(caller, correlation, deadline)
                .map_err(PublicationError::from)
        });
        let result = self.finish_call_publication(result);
        let result = crate::app_session::collapse_app_results([
            result,
            self.service_compat_writer_progress(),
            self.service_dialog_writer_progress(),
        ]);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_compat_event_state(
            "call.exit",
            Some((correlation.0, if result.is_ok() { "ok" } else { "error" })),
        );
        result
    }

    fn prepare_compat_call_answer(
        &self,
        caller: &PrimaryCallerContext,
        target: &Arc<WriterLink>,
        correlation: CorrelationId,
        answer: CompatCallAnswer,
        deadline: Instant,
    ) -> Result<PreparedFrame, HostAppError> {
        let channel = keld_ipc::channel_table::COMPAT_CONTROL.id();
        let control = match &answer {
            CompatCallAnswer::Released => Some(ControlMessage::CompatReleased { correlation }),
            CompatCallAnswer::Stale => Some(ControlMessage::CompatTerminal {
                correlation,
                disposition: ValueFreeTerminal::Stale,
            }),
            CompatCallAnswer::Capacity => Some(ControlMessage::Capacity {
                channel,
                correlation,
            }),
            CompatCallAnswer::Unsubscribed | CompatCallAnswer::Denied(_) => None,
        };
        if let Some(control) = control {
            return self.prepare_primary_control(
                caller,
                Arc::clone(target),
                control,
                deadline,
                "compat fixed reply preparation",
            );
        }
        // Every diagnostic remains VALUE even if its particular string is short.
        let reservation =
            self.reserve_primary_value(caller, deadline, "compat error VALUE reservation")?;
        let error = match answer {
            CompatCallAnswer::Unsubscribed => {
                refused("current authenticated primary role has no compat subscription")
            }
            CompatCallAnswer::Denied(reason) => CallError::from(&reason),
            _ => {
                return Err(app_detail(
                    "compat response",
                    "fixed response escaped closed classification",
                ));
            }
        };
        reservation.encode_error_value(Arc::clone(target), channel, correlation, &error)
    }
}
