//! Admission of the existing fixed numeric operation. The actual call receiver
//! and the actual token are attached before an admitted write can block/fail.
use super::{
    APP_LINK_IO_DEADLINE, Arc, Call, ECHO_CHANNEL, FrameKind, HostAppError, Instant,
    NativeNumericResultReservation, Ordering, Pending, PendingPrimaryEcho, PrimaryEchoCall,
    PrimaryRouterHandle, RendererBridgeRequest, State, accounting, app_detail, mpsc,
    primary_writer, same_caller, with_primary_caller_locked,
};

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep reservation, exact caller admission and same pending-record transfer in one closed operation."
    )]
    pub(in crate::app_session) fn begin_numeric_echo(
        &self,
        request: &RendererBridgeRequest,
        storage: &mut Option<NativeNumericResultReservation>,
        call: &mut Option<Call>,
    ) -> Result<(), HostAppError> {
        if call.is_some() {
            return Err(app_detail(
                "numeric Echo admission",
                "call receiver is already attached",
            ));
        }
        let held = storage.as_ref().ok_or_else(|| {
            app_detail(
                "numeric Echo admission",
                "original native result storage is absent",
            )
        })?;
        let lease = request.document_lease().ok_or_else(|| {
            app_detail(
                "numeric Echo admission",
                "original document lease is absent",
            )
        })?;
        if request.channel() != keld_ipc::channel_table::ECHO.wire_id()
            || request.payload().len() > held.maximum_payload_len()
            || !held.matches_request(
                request.webview(),
                request.navigation(),
                request.request(),
                lease,
            )
        {
            return Err(app_detail(
                "numeric Echo admission",
                "request does not match its storage",
            ));
        }
        accounting::require_prepaid(held)?;
        let deadline = Instant::now() + APP_LINK_IO_DEADLINE;
        let target = self.current_primary_writer_target()?.ok_or_else(|| {
            app_detail(
                "numeric Echo admission",
                "no admitted primary generation is available",
            )
        })?;
        let caller = target.caller();
        if request
            .role_witness()
            .is_none_or(|role| !role.matches(&caller.reader_stop))
        {
            return Err(app_detail(
                "numeric Echo admission",
                "original role no longer owns this call",
            ));
        }
        let correlation = self.mint_host_correlation();
        let frame = self
            .reserve_primary_value(caller, deadline, "numeric Echo reservation")?
            .copy_value(
                Arc::clone(&target),
                FrameKind::Call,
                ECHO_CHANNEL,
                correlation,
                request.payload(),
            )?;
        // The same native allowance was checked before this allocation. Its
        // actual receiver remains in the attached dispatcher even if admission
        // or the later physical write fails.
        let (reply, receiver) = mpsc::sync_channel(1);
        *call = Some(Call {
            inner: PrimaryEchoCall {
                correlation,
                reply: receiver,
            },
            notice: None,
            caller: caller.clone(),
        });
        let mut prepared = Some(frame);
        let mut receipt = None;
        let admitted =
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
                let active = current.as_ref().ok_or_else(|| {
                    app_detail("numeric Echo admission", "validated role disappeared")
                })?;
                let mut pending = self.pending_echo.lock().map_err(|_| {
                    app_detail("numeric Echo admission", "pending-call lock poisoned")
                })?;
                if pending.is_some() {
                    return Err(app_detail(
                        "numeric Echo admission",
                        "another call is pending",
                    ));
                }
                let actual = storage.take().ok_or_else(|| {
                    app_detail(
                        "numeric Echo admission",
                        "original result storage disappeared",
                    )
                })?;
                // Prepared is still the SAME existing pending source, not an
                // admitted primary frame. Attach before enqueue may fail/panic.
                *pending = Some(PendingPrimaryEcho {
                    attempt: caller.attempt,
                    correlation,
                    reply,
                    numeric: Some(Pending {
                        caller: caller.clone(),
                        state: State::prepared(actual),
                    }),
                });
                let frame = prepared.take().ok_or_else(|| {
                    app_detail("numeric Echo admission", "prepared frame disappeared")
                })?;
                match primary_writer::admit_locked(active, frame) {
                    Ok(actual_receipt) => {
                        if let Some(waiter) = pending.as_mut()
                            && let Some(numeric) = waiter.numeric.as_mut()
                        {
                            numeric.state.admitted();
                        }
                        self.pending_echo_attempt
                            .store(caller.attempt, Ordering::Release);
                        self.pending_echo_corr
                            .store(correlation.0, Ordering::Release);
                        receipt = Some(actual_receipt);
                        Ok(())
                    }
                    Err(error) => {
                        if let Some(waiter) = pending.as_mut()
                            && let Some(numeric) = waiter.numeric.as_mut()
                        {
                            numeric.retire("application call was not admitted");
                        }
                        Err(error)
                    }
                }
            });
        // Error paths keep the exact token either in storage (not transferred)
        // or in Pending; the staged call receiver is never returned in an error.
        if let Err(error) = admitted {
            let mut pending = self.pending_echo.lock().map_err(|_| {
                app_detail(
                    "numeric Echo admission",
                    "pending-call lock poisoned after refusal",
                )
            })?;
            if let Some(waiter) = pending
                .as_mut()
                .filter(|waiter| waiter.correlation == correlation)
                && let Some(numeric) = waiter
                    .numeric
                    .as_mut()
                    .filter(|numeric| same_caller(&numeric.caller, caller))
            {
                numeric.retire("application call was not admitted");
                numeric.notify(&waiter.reply);
            }
            return Err(error);
        }
        let receipt = receipt.ok_or_else(|| {
            app_detail(
                "numeric Echo admission",
                "original role retired before admission",
            )
        })?;
        self.await_primary_write(&target, receipt, deadline, "numeric Echo call write")
    }
}
