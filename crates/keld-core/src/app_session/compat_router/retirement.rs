//! Fixed metadata for the one existing native delivery's retirement reply.
//! This snapshot is not authority: admission repeats the actual same record/caller
//! check under the enclosing canonical retirement cut, before moving its frame.

use super::{
    ActivePrimaryGeneration, Arc, Charge, CompatSession, ConsumerDisposition, CorrelationId,
    Direction, HostAppError, NativeRouteKey, PrimaryCallerContext, PrimaryRouterHandle, app_detail,
    compat_control, same_caller,
};
use crate::app_session::primary_writer::{
    self, ControlMessage, PreparedFrame, ValueFreeTerminal, WriteReceipt, WriterLink,
};

#[derive(Clone)]
pub(in crate::app_session) struct CompatRetirementReply {
    caller: PrimaryCallerContext,
    key: NativeRouteKey,
    correlation: CorrelationId,
    disposition: ValueFreeTerminal,
}

impl CompatRetirementReply {
    /// Compare bounded snapshots only; final admission still requires the same
    /// actual ledger/caller check at the canonical retirement cut.
    pub(in crate::app_session) fn same_record(&self, other: &Self) -> bool {
        same_caller(&self.caller, &other.caller)
            && self.key == other.key
            && self.correlation == other.correlation
            && self.disposition == other.disposition
    }
}

/// The only constructor preserves the original fixed value-free terminal.
/// It cannot carry another CONTROL variant or an application payload.
pub(in crate::app_session) struct PreparedCompatRetirement {
    expected: CompatRetirementReply,
    frame: PreparedFrame,
}

impl PrimaryRouterHandle {
    /// Windows normal retirement performs no primary I/O in the canonical cut.
    /// The enclosing owner has already admitted its fixed terminal snapshot and
    /// retains both its unique receipt and the exact A close handle outside it.
    pub(in crate::app_session) fn retire_compat_locked(
        &self,
        active: &mut ActivePrimaryGeneration,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        let caller = PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        // Fixed ready:false continuation is published after canonical revocation
        // by the same lifecycle source, outside this lock and under its turn.
        (|| {
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat retirement", "session lock poisoned"))?;
            if state
                .subscribed
                .as_ref()
                .is_some_and(|owner| same_caller(owner, &caller))
            {
                state.subscribed = None;
            }
            state.ledger.role_retired(&caller);
            let mut retained_selection = false;
            for charge in state.ledger.data.iter_mut().flatten() {
                if charge.owned_by(&caller) {
                    if charge.has_unadmitted_terminal() {
                        retained_selection = true;
                    } else {
                        // No selected source is erased. This alone is no native fact.
                        charge.pending_reply = None;
                    }
                }
            }
            if retained_selection {
                return Err(app_detail(
                    "compat retirement",
                    "selected original terminal remains retained; no replacement terminal is permitted",
                ));
            }
            if state.ledger.data[Direction::RendererToMain.index()]
                .as_ref()
                .is_some_and(|charge| charge.owned_by(&caller))
            {
                return Err(app_detail(
                    "compat retired native terminal",
                    "exact pending-record native settlement authority is not yet qualified",
                ));
            }
            Ok(())
        })()
    }

    /// Caller retains transition/current for this exact original generation.
    pub(in crate::app_session) fn compat_retirement_reply_locked(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Result<Option<CompatRetirementReply>, HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(None);
        };
        let state = compat
            .lock()
            .map_err(|_| app_detail("compat retirement snapshot", "session lock poisoned"))?;
        let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_ref() else {
            return Ok(None);
        };
        if !charge.owned_by(caller)
            || charge.primary_write.is_some()
            || charge.retirement_write.is_some()
        {
            return Ok(None);
        }
        let Some(correlation) = charge.pending_reply else {
            return Ok(None);
        };
        let disposition = fixed_retirement_terminal(charge)?;
        Ok(Some(CompatRetirementReply {
            caller: caller.clone(),
            key: charge.key,
            correlation,
            disposition,
        }))
    }

    /// Encode only the same fixed retirement terminal in an already charged
    /// CONTROL position. Accepted shutdown tails keep their original deadline;
    /// the reservation carries storage, never authority to publish to a role.
    pub(in crate::app_session) fn prepare_compat_retirement_reply_reserved(
        expected: CompatRetirementReply,
        target: Arc<WriterLink>,
        reservation: primary_writer::Reservation,
    ) -> Result<PreparedCompatRetirement, HostAppError> {
        if !target.matches(&expected.caller) {
            return Err(app_detail(
                "compat retirement preparation",
                "target does not belong to the original caller",
            ));
        }
        let frame = ControlMessage::CompatTerminal {
            correlation: expected.correlation,
            disposition: expected.disposition,
        }
        .encode(reservation, target)?;
        Ok(PreparedCompatRetirement { expected, frame })
    }

    /// Caller still owns the original transition/current retirement cut and
    /// validates either its live role or its exact already accepted tail cause.
    /// Stopping reader admission during accepted Quit is not physical retirement.
    /// A completed or already-admitted original response is never duplicated.
    pub(in crate::app_session) fn admit_compat_retirement_reply_locked(
        &self,
        active: &ActivePrimaryGeneration,
        prepared: PreparedCompatRetirement,
    ) -> Result<Option<WriteReceipt>, HostAppError> {
        let PreparedCompatRetirement { expected, frame } = prepared;
        if active.attempt != expected.caller.attempt
            || active.identity != expected.caller.identity
            || !Arc::ptr_eq(&active.reader_stop, &expected.caller.reader_stop)
        {
            return Ok(None);
        }
        let Some(compat) = &self.compat else {
            return Ok(None);
        };
        let mut state = compat
            .lock()
            .map_err(|_| app_detail("compat retirement admission", "session lock poisoned"))?;
        let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_mut() else {
            return Ok(None);
        };
        if charge.key != expected.key
            || !charge.owned_by(&expected.caller)
            || charge.pending_reply != Some(expected.correlation)
            || charge.primary_write.is_some()
            || charge.retirement_write.is_some()
        {
            return Ok(None);
        }
        if fixed_retirement_terminal(charge)? != expected.disposition {
            return Ok(None);
        }
        let receipt = primary_writer::admit_locked(active, frame)?;
        // Infallible terminal commit immediately after final successful enqueue.
        charge.pending_reply = None;
        charge.retired = true;
        charge.retirement_write = Some(receipt.observation());
        // Any temporarily moved source strings remain charged until that exact
        // source owner restores/drops them; do not clear terminal_preparing here.
        if let Some(observation) = charge.retirement_write.as_ref() {
            observation
                .register_quiescence_interest()
                .map_err(|error| error.into_host_error("compat retirement completion interest"))?;
        }
        Ok(Some(receipt))
    }
}

impl CompatSession {
    /// No wire reply and no role selection. Only the existing record's actual
    /// consumer/native witnesses and original physical write can release storage.
    pub(super) fn release_quiescent_retired_terminal(&mut self) -> Result<(), HostAppError> {
        let Some(charge) = self.ledger.data[Direction::MainToRenderer.index()].as_mut() else {
            return Ok(());
        };
        if !charge.retired
            || !charge.event_obligations_settled()
            || charge.terminal_preparing
            || charge.primary_write.is_some()
            || charge.pending_reply.is_some()
        {
            return Ok(());
        }
        if let Some(observation) = charge.retirement_write.as_ref() {
            if !observation
                .is_quiesced()
                .map_err(|error| error.into_host_error("compat retirement completion"))?
            {
                observation
                    .register_quiescence_interest()
                    .map_err(|error| {
                        error.into_host_error("compat retirement completion interest")
                    })?;
                return Ok(());
            }
            charge.retirement_write = None;
        }
        self.release_completed_native_event()
    }
}

/// One canonical enum selection; diagnostic bytes stay in their existing source.
fn fixed_retirement_terminal(charge: &Charge) -> Result<ValueFreeTerminal, HostAppError> {
    if charge.terminal_preparing {
        return Err(app_detail(
            "compat retirement snapshot",
            "the original terminal source is preparing and remains retained",
        ));
    }
    if charge.consumer_done {
        return match charge.terminal.as_ref() {
            Some(ConsumerDisposition::Consumed) => Ok(ValueFreeTerminal::Consumed),
            Some(ConsumerDisposition::NoListener) => Ok(ValueFreeTerminal::NoListener),
            Some(ConsumerDisposition::Refused { .. }) | None => Err(app_detail(
                "compat retirement snapshot",
                "selected original diagnostic/source cannot be replaced by a fixed retirement",
            )),
        };
    }
    Ok(ValueFreeTerminal::Retired(
        charge
            .native_retirement
            .unwrap_or(compat_control::DispatchCertainty::OutcomeUnknown),
    ))
}
