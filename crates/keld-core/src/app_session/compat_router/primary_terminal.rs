//! A settled native record retries the sole primary writer without a second queue.
//! Source strings move out temporarily under the SAME DATA charge; Full restores
//! them, final admission commits its receipt, and actual completion releases it.

use super::super::primary_writer::{
    ControlMessage, RetainedWrite, ValueFreeTerminal, WriterRefusal,
};
use super::{
    Arc, Charge, CompatControlResponse, CompatSession, ConsumerDisposition, CorrelationId,
    Direction, FrameKind, HostAppError, MAX_CONTROL_BYTES, Mutex, NativeRouteKey,
    PrimaryCallerContext, PrimaryRouterHandle, TerminalDisposition, app_detail, app_ipc,
    compat_control, terminal, with_primary_caller_locked,
};
#[cfg(test)]
use super::{CallError, RendererBridgeOutcome};
use std::sync::mpsc::TrySendError;
use std::time::Instant;

#[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
mod selected_observation;

struct TerminalSource {
    owner: Arc<Mutex<CompatSession>>,
    caller: PrimaryCallerContext,
    key: NativeRouteKey,
    correlation: CorrelationId,
    deadline: Instant,
    response: Option<CompatControlResponse>,
    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
    source_attempt: Option<u32>,
    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
    drop_restoration_observed: bool,
}
impl TerminalSource {
    fn take(owner: &Arc<Mutex<CompatSession>>) -> Result<Option<Self>, HostAppError> {
        let mut state = owner
            .lock()
            .map_err(|_| app_detail("compat settled terminal", "session lock poisoned"))?;
        let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_mut() else {
            return Ok(None);
        };
        if !charge.event_obligations_settled()
            || (charge.retired && charge.native_retirement.is_none())
            || charge.terminal_preparing
            || charge.primary_write.is_some()
            || charge.retirement_write.is_some()
        {
            return Ok(None);
        }
        let Some(correlation) = charge.pending_reply else {
            return Ok(None);
        };
        let Some(deadline) = charge.terminal_deadline else {
            return Ok(None);
        };
        let disposition = if charge.consumer_done {
            let Some(disposition) = charge.terminal.take() else {
                return Ok(None);
            };
            terminal(disposition)
        } else if let Some(certainty) = charge.native_retirement {
            TerminalDisposition::RouteRetired { certainty }
        } else {
            return Ok(None);
        };
        charge.terminal_preparing = true;
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        {
            // Exhaustion stays diagnostic-only and never reuses a prior attempt.
            charge.selected_source_sequence =
                charge.selected_source_sequence.saturating_add(1).min(129);
        }
        Ok(Some(Self {
            owner: Arc::clone(owner),
            caller: charge.owner.clone(),
            key: charge.key,
            correlation,
            deadline,
            response: Some(CompatControlResponse::Deliver { disposition }),
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            source_attempt: selected_observation::source_attempt(charge),
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            drop_restoration_observed: false,
        }))
    }

    fn matches(&self, charge: &Charge) -> bool {
        charge.key == self.key && charge.owned_by(&self.caller) && charge.terminal_preparing
    }

    fn fixed(&self) -> Option<ValueFreeTerminal> {
        match self.response.as_ref()? {
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::Consumed,
            } => Some(ValueFreeTerminal::Consumed),
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::NoListener,
            } => Some(ValueFreeTerminal::NoListener),
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::RouteRetired { certainty },
            } => Some(ValueFreeTerminal::Retired(*certainty)),
            _ => None,
        }
    }

    fn restore(&mut self, charge: &mut Charge) {
        if !self.matches(charge) {
            return;
        }
        let Some(response) = self.response.take() else {
            return;
        };
        let original = match response {
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::RouteRetired { certainty },
            } if charge.native_retirement == Some(certainty) && !charge.consumer_done => {
                charge.terminal_preparing = false;
                return; // the SAME sealed proof remains in its original record
            }
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::Consumed,
            } => ConsumerDisposition::Consumed,
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::NoListener,
            } => ConsumerDisposition::NoListener,
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::Refused { error },
            } => ConsumerDisposition::Refused { error },
            // This private owner only moves the three original consumer variants.
            // Refuse to clear custody if that invariant is ever broken.
            _ => return,
        };
        charge.terminal = Some(original);
        charge.terminal_preparing = false;
    }
    fn restore_on_drop(&mut self, observe: impl FnOnce(&CompatSession)) {
        if self.response.is_none() {
            return;
        }
        let owner = Arc::clone(&self.owner);
        if let Ok(mut state) = owner.lock()
            && let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_mut()
        {
            self.restore(charge);
            // If retirement canceled this never-admitted preparation, only now
            // are its moved strings back under the original physical record.
            let _ = state.release_quiescent_retired_terminal();
            observe(&state);
        }
    }
}
impl Drop for TerminalSource {
    fn drop(&mut self) {
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if self.drop_restoration_observed {
            return;
        }
        self.restore_on_drop(|_| {});
    }
}

impl PrimaryRouterHandle {
    // Diagnostic scalars only: no owner, source string, payload or lease is copied.
    // The compat guard and the bounded diagnostic guard both end before printing.
    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn observe_compat_event_state(
        &self,
        phase: &'static str,
        call: Option<(u32, &'static str)>,
    ) {
        use std::io::Write as _;
        type Snapshot = Option<(NativeRouteKey, Option<u32>, [bool; 10], Option<bool>)>;
        static OBSERVED: Mutex<(usize, Option<Snapshot>)> = Mutex::new((0, None));
        if std::env::var_os(keld_wv::TEST_WINDOWS_T2_OBSERVE_ENV).as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let snapshot = {
            let Some(compat) = &self.compat else {
                return;
            };
            let Ok(state) = compat.lock() else {
                return;
            };
            state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .map(|charge| {
                    (
                        charge.key,
                        charge.pending_reply.map(|value| value.0),
                        [
                            charge.consumer_done,
                            charge.native_done,
                            charge.terminal_preparing,
                            charge.primary_write.is_some(),
                            charge.retirement_write.is_some(),
                            charge.event_release_sent,
                            charge.retired,
                            charge.native_retirement.is_some(),
                            charge.terminal.is_some(),
                            charge.event_obligations_settled(),
                        ],
                        charge
                            .native_storage
                            .as_ref()
                            .map(|released| released.load(std::sync::atomic::Ordering::Acquire)),
                    )
                })
        };
        let sequence = {
            let Ok(mut observed) = OBSERVED.lock() else {
                return;
            };
            // Service may run in a drain loop. Emit only changed charge facts or
            // a real CALL milestone, and cap this diagnostic at 128 rows per Host.
            if observed.0 >= 128 || (call.is_none() && observed.1 == Some(snapshot)) {
                return;
            }
            observed.0 += 1;
            observed.1 = Some(snapshot);
            observed.0
        };
        let charge = snapshot.map(|(key, pending_reply, facts, storage_released)| {
            serde_json::json!({
                "webview": key.webview, "navigation": key.navigation, "message": key.message,
                "pending_reply": pending_reply,
                "consumer_done": facts[0], "native_done": facts[1],
                "terminal_preparing": facts[2], "primary_write": facts[3],
                "retirement_write": facts[4], "event_release_sent": facts[5],
                "retired": facts[6], "native_retirement": facts[7],
                "terminal_present": facts[8], "obligations_settled": facts[9],
                "native_storage_released": storage_released,
            })
        });
        let value = serde_json::json!({
            "sequence": sequence, "phase": phase,
            "correlation": call.map(|value| value.0),
            "call_state": call.map(|value| value.1), "charge": charge,
        });
        let _ = writeln!(
            std::io::stderr().lock(),
            "KELD_COMPAT_EVENT_DIAGNOSTIC {value}"
        );
    }

    // Each row comes from an actual guarded transition, never a later snapshot.
    // Printing happens after all publication guards; order is not a witness.
    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
    fn observe_compat_event_completion(phase: &'static str, key: NativeRouteKey) {
        use std::io::Write as _;
        if std::env::var_os(keld_wv::TEST_WINDOWS_T2_OBSERVE_ENV).as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let value = serde_json::json!({
            "phase": phase, "webview": key.webview,
            "navigation": key.navigation, "message": key.message,
        });
        let _ = writeln!(
            std::io::stderr().lock(),
            "KELD_COMPAT_EVENT_COMPLETION {value}"
        );
    }

    /// Called without transition/current/Registry held. The wake is only a hint:
    /// every effect comes from one existing retained record and its exact caller.
    #[expect(
        clippy::too_many_lines,
        reason = "One retained terminal source preserves its restore, admission and completion order across writer capacity."
    )]
    pub(in crate::app_session) fn service_compat_writer_progress(
        &self,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_compat_event_state("writer.enter", None);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_x11_pressure_progress("writer.enter");
        self.finish_compat_primary_receipt(compat)?;
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_compat_event_state("writer.after-receipt", None);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_x11_pressure_progress("writer.after-receipt");
        let Some(mut source) = TerminalSource::take(compat)? else {
            return Ok(());
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_compat_event_state("writer.preparing", None);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let selected_diagnostic = source.selected_diagnostic();
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_selected_diagnostic("source-selected", selected_diagnostic, false, None, None);
        compat_control::validate_response(source.response.as_ref().ok_or_else(|| {
            app_detail("compat terminal validation", "source custody disappeared")
        })?)
        .map_err(|source| app_ipc("compat terminal validation", &source))?;
        let Some(target) = self.primary_writer_target(&source.caller)? else {
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            if let Some(restored) = source.finish_denied_target() {
                self.observe_restored_selected_source(&restored);
            }
            #[cfg(not(all(windows, feature = "profile-test-hooks", debug_assertions)))]
            drop(source);
            return self.finish_compat_primary_receipt(compat);
        };
        let writer = self.primary_writer_owner()?;
        let fixed = source.fixed();
        let reservation = if fixed.is_some() {
            writer.try_reserve_control(source.deadline)
        } else {
            writer.try_reserve_value(source.deadline)
        };
        let reservation = match reservation {
            Ok(reservation) => reservation,
            Err(WriterRefusal::Full) => {
                self.restore_compat_interest(&mut source, fixed.is_some())?;
                drop(source);
                #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
                self.observe_selected_diagnostic(
                    "value-full-restored",
                    selected_diagnostic,
                    true,
                    None,
                    None,
                );
                return self.finish_compat_primary_receipt(compat);
            }
            Err(error) => return Err(error.into_host_error("compat terminal reservation")),
        };
        // Counting and encoding occur outside every canonical caller/native
        // fence. Only this existing source owner temporarily holds the strings.
        let frame = if let Some(disposition) = fixed {
            ControlMessage::CompatTerminal {
                correlation: source.correlation,
                disposition,
            }
            .encode(reservation, Arc::clone(&target))?
        } else {
            reservation.encode_value(
                Arc::clone(&target),
                FrameKind::Reply,
                keld_ipc::channel_table::COMPAT_CONTROL.id(),
                source.correlation,
                source.response.as_ref().ok_or_else(|| {
                    app_detail("compat terminal encoding", "source custody disappeared")
                })?,
                MAX_CONTROL_BYTES,
            )?
        };
        let mut prepared = Some(frame);
        let mut disconnected = false;
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let mut consumed_admitted = None;
        let caller = source.caller.clone();
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |current| {
            let active = current.as_ref().ok_or_else(|| {
                app_detail("compat terminal admission", "validated role disappeared")
            })?;
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat terminal admission", "session lock poisoned"))?;
            let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_mut() else {
                return Ok(());
            };
            if !source.matches(charge)
                || (charge.retired && charge.native_retirement.is_none())
                || charge.pending_reply != Some(source.correlation)
                || !charge.event_obligations_settled()
                || charge.primary_write.is_some()
            {
                return Ok(());
            }
            let Some(frame) = prepared.take() else {
                return Ok(());
            };
            if !active
                .writer_link
                .as_ref()
                .is_some_and(|link| frame.targets(link))
            {
                return Err(app_detail(
                    "compat terminal admission",
                    "prepared target is not the original link",
                ));
            }
            if Instant::now() >= source.deadline {
                return Err(WriterRefusal::Deadline.into_host_error("compat terminal admission"));
            }
            match frame.try_send() {
                Ok(receipt) => {
                    // Infallible exact-record commit immediately follows the
                    // final successful try_send under the same caller/record cut.
                    charge.pending_reply = None;
                    charge.terminal_preparing = false;
                    charge.primary_write = Some(RetainedWrite::new(receipt));
                    source.response = None; // source bytes are now physically freed
                    if let Some(receipt) = charge.primary_write.as_ref() {
                        receipt.register_completion_interest().map_err(|error| {
                            error.into_host_error("compat terminal completion interest")
                        })?;
                    }
                    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
                    if fixed == Some(ValueFreeTerminal::Consumed)
                        && !charge.retired
                        && charge.native_retirement.is_none()
                    {
                        consumed_admitted = Some(charge.key);
                    }
                }
                Err(TrySendError::Full(frame)) => {
                    drop(frame); // only this attempted downstream reservation
                    source.restore(charge);
                    if fixed.is_some() {
                        writer.register_control_interest()
                    } else {
                        writer.register_value_interest()
                    }
                    .map_err(|error| error.into_host_error("compat terminal capacity interest"))?;
                }
                Err(TrySendError::Disconnected(frame)) => {
                    drop(frame); // returned frame was never admitted/written
                    source.restore(charge);
                    disconnected = true;
                }
            }
            Ok(())
        })?;
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if let Some(key) = consumed_admitted {
            Self::observe_compat_event_completion("consumed-reply-admitted", key);
        }
        drop(source); // an unadmitted source returns to its same record here
        if disconnected {
            self.retire_generation_after_write_failure(&caller)?;
            return Err(WriterRefusal::Disconnected.into_host_error("compat terminal writer owner"));
        }
        let result = self.finish_compat_primary_receipt(compat);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_compat_event_state("writer.exit", None);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_x11_pressure_progress("writer.exit");
        result
    }

    fn restore_compat_interest(
        &self,
        source: &mut TerminalSource,
        control: bool,
    ) -> Result<(), HostAppError> {
        let caller = source.caller.clone();
        let owner = Arc::clone(&source.owner);
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |_| {
            let mut state = owner
                .lock()
                .map_err(|_| app_detail("compat terminal interest", "session lock poisoned"))?;
            let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_mut() else {
                return Ok(());
            };
            if !source.matches(charge) || (charge.retired && charge.native_retirement.is_none()) {
                return Ok(());
            }
            // Do not refund the settled upstream DATA source on downstream Full.
            // Restore its exact strings before registering the coalesced wake.
            source.restore(charge);
            let writer = self.primary_writer_owner()?;
            if control {
                writer.register_control_interest()
            } else {
                writer.register_value_interest()
            }
            .map_err(|error| error.into_host_error("compat terminal capacity interest"))
        })
    }

    /// Only observe/drop an actual completed receipt. DATA/native release and
    /// exact failure reporting remain attached to this same charge for service.
    pub(in crate::app_session) fn settle_compat_primary_receipt(
        &self,
    ) -> Result<bool, WriterRefusal> {
        let Some(compat) = &self.compat else {
            return Ok(false);
        };
        let mut state = compat.lock().map_err(|_| WriterRefusal::Poisoned)?;
        let mut changed = false;
        for direction in [Direction::RendererToMain, Direction::MainToRenderer] {
            let Some(charge) = state.ledger.data[direction.index()].as_mut() else {
                continue;
            };
            if direction == Direction::MainToRenderer
                && (!charge.event_obligations_settled()
                    || charge.terminal_preparing
                    || charge.retirement_write.is_some())
            {
                continue;
            }
            if let Some(written) = &mut charge.primary_write {
                changed |= written.settle()?;
            }
        }
        Ok(changed)
    }

    fn finish_renderer_primary_receipt(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
    ) -> Result<(), HostAppError> {
        let failed = {
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat renderer completion", "ledger lock poisoned"))?;
            let Some(charge) = state.ledger.data[Direction::RendererToMain.index()].as_mut() else {
                return Ok(());
            };
            let Some(failed) = RetainedWrite::take_completed(&mut charge.primary_write) else {
                return Ok(());
            };
            state.ledger.clear_if_complete(Direction::RendererToMain);
            failed
        };
        failed.map_or(Ok(()), |failure| self.report_primary_write_failure(failure))
    }

    fn finish_compat_primary_receipt(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
    ) -> Result<(), HostAppError> {
        self.settle_compat_primary_receipt()
            .map_err(|error| error.into_host_error("compat terminal completion"))?;
        self.finish_renderer_primary_receipt(compat)?;
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let mut cleared = None;
        let (failed, published) = {
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat terminal completion", "session lock poisoned"))?;
            state.release_quiescent_retired_terminal()?;
            // The same source-progress turn also reaps CONTROL after its actual
            // application, native storage and physical-write obligations ended.
            // Do this before the DATA-only early return; the existing predicate
            // remains the sole authority for releasing each original charge.
            state.ledger.reap_native_controls();
            let Some(charge) = state.ledger.data[Direction::MainToRenderer.index()].as_mut() else {
                return Ok(());
            };
            if !charge.event_obligations_settled()
                || charge.terminal_preparing
                || charge.retirement_write.is_some()
            {
                return Ok(());
            }
            let failed = match RetainedWrite::take_completed(&mut charge.primary_write) {
                Some(failed) => failed,
                None if charge.primary_write.is_some() => return Ok(()),
                None => None,
            };
            if charge.pending_reply.is_some() {
                return Ok(());
            }
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            let normal_key = (!charge.retired
                && charge.native_retirement.is_none()
                && charge.consumer_done
                && charge.native_done
                && charge.native_storage.is_some()
                && failed.is_none())
            .then_some(charge.key);
            let published = state.release_completed_native_event();
            #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
            if published.is_ok() && !state.ledger.occupied(Direction::MainToRenderer) {
                cleared = normal_key;
            }
            (failed, published)
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if let Some(key) = cleared {
            Self::observe_compat_event_completion("native-data-cleared", key);
        }
        // Once extracted, the actual failure survives an outcome-send error.
        // Recovery is still outside every source/role/Window publication guard.
        let reported = failed.map_or(Ok(()), |failure| self.report_primary_write_failure(failure));
        super::super::collapse_app_results([published, reported])
    }
}

#[cfg(test)]
mod tests;
