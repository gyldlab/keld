//! Renderer DATA uses the existing DATA source record and the one VALUE position.
use super::native_input::{borrowed_renderer_event, preflight_matches};
use super::{
    Arc, CallError, CompatSession, CorrelationId, Direction, FrameKind, HostAppError, Instant,
    Mutex, NativePublicationDisposition, NativeRouteKey, PrimaryCallerContext, PrimaryRouterHandle,
    PublicationResult, RendererBridgeOutcome, RendererBridgeRequest, RendererChannelQuery,
    RendererRoute, WebviewId, WvTerminalDisposition, admit_locked, app_detail, app_ipc,
    compat_control, dispatch_privileged_query, preflight_role_matches, principal, refused,
    same_caller, same_route, with_primary_caller_locked,
};
use crate::app_session::primary_writer::RetainedWrite;

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep one original preflight, DATA reservation and final writer commit with explicit failure custody."
    )]
    pub(super) fn send_renderer_owned(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
        request: &RendererBridgeRequest,
        key: NativeRouteKey,
        channel: &str,
        original_call: u32,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let caller = compat
            .lock()
            .map_err(|_| app_detail("compat Send source", "ledger lock poisoned"))?
            .ledger
            .preflight
            .as_ref()
            .map(|pending| pending.caller.clone());
        // No current-successor fallback and no invented retired terminal. The
        // native original pending record remains responsible for its own drain.
        let Some(caller) = caller else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let mut route = None;
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |_| {
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows
                .lock()
                .map_err(|_| app_detail("compat Send source", "Registry lock poisoned"))?;
            if preflight_role_matches(&caller, request.role_witness()) {
                route = Self::checked_request_route(&windows, &caller, request, key);
            }
            Ok(())
        })?;
        let Some(route) = route else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let result = (|| -> PublicationResult<()> {
            let Some(target) = self.primary_writer_target(&caller)? else {
                return Ok(());
            };
            let mut control = self.reserve_native_send_control(&caller, route.window, deadline)?;
            if control.is_none() {
                return Ok(());
            }
            let reservation =
                self.reserve_primary_value(&caller, deadline, "compat renderer VALUE reservation")?;
            let event = borrowed_renderer_event(request, &route, key, channel, request.payload())?;
            let retained = keld_ipc::codec::encoded_len(&event)
                .map_err(|source| app_ipc("compat renderer event count", &source))?;
            let mut frame = Some(reservation.encode_value(
                Arc::clone(&target),
                FrameKind::Event,
                keld_ipc::channel_table::COMPAT_CONTROL.id(),
                CorrelationId(0),
                &event,
                compat_control::COMPAT_DATA_FRAME_BYTES,
            )?);
            let mut refusal = None;
            with_primary_caller_locked(&self.current, &self.shutdown, &caller, |current| {
                let Some(windows) = &self.windows else {
                    return Ok(());
                };
                let windows = windows.lock().map_err(|_| {
                    app_detail("compat renderer admission", "Registry lock poisoned")
                })?;
                if !same_route(&windows, &caller, &route) {
                    return Ok(());
                }
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("compat renderer admission", "ledger lock poisoned"))?;
                if !state.subscribed_to(&caller)
                    || !state.ledger.preflight.as_ref().is_some_and(|pending| {
                        preflight_matches(pending, &caller, original_call, &route.document, channel)
                    })
                {
                    refusal = Some(refused(
                        "original send no longer has its admitted role and document",
                    ));
                    return Ok(());
                }
                let active = current.as_ref().ok_or_else(|| {
                    app_detail("compat renderer admission", "validated caller disappeared")
                })?;
                let CompatSession {
                    verified, ledger, ..
                } = &mut *state;
                let guarded = dispatch_privileged_query(
                    verified.manifest(),
                    principal(&route),
                    RendererChannelQuery {
                        channel,
                        current_window: verified
                            .manifest()
                            .window_channel_grants(route.window_key()),
                    },
                    |_| {
                        if !route.document.is_current() {
                            return Ok(());
                        }
                        ledger
                            .reserve(Direction::RendererToMain, key, &caller, retained)
                            .map_err(|_| {
                                app_detail(
                                    "compat renderer admission",
                                    "bounded renderer DATA lane is occupied",
                                )
                            })?;
                        let prepared = frame.take().ok_or_else(|| {
                            app_detail("compat renderer admission", "prepared frame disappeared")
                        })?;
                        let charge = ledger.data[Direction::RendererToMain.index()]
                            .as_mut()
                            .ok_or_else(|| {
                                app_detail(
                                    "compat renderer admission",
                                    "reserved DATA source disappeared",
                                )
                            })?;
                        charge.document = Some(route.document.clone());
                        charge.native_window = Some(route.window);
                        let receipt = match admit_locked(active, prepared) {
                            Ok(receipt) => receipt,
                            Err(error) => {
                                ledger.data[Direction::RendererToMain.index()] = None;
                                return Err(error);
                            }
                        };
                        // No fallible operation between enqueue and the same source's
                        // unique receipt commit. Worker completion retains this slot.
                        charge.primary_write = Some(RetainedWrite::new(receipt));
                        ledger.preflight = None;
                        if let Some(written) = ledger.data[Direction::RendererToMain.index()]
                            .as_ref()
                            .and_then(|charge| charge.primary_write.as_ref())
                        {
                            written.register_completion_interest().map_err(|error| {
                                error.into_host_error("compat renderer completion interest")
                            })?;
                        }
                        Ok(())
                    },
                );
                match guarded {
                    Ok(result) => result,
                    Err(reason) => {
                        refusal = Some(CallError::from(&reason));
                        Ok(())
                    }
                }
            })?;
            drop(frame);
            if let Some(error) = refusal {
                self.publish_renderer_refusal(
                    compat,
                    &caller,
                    &route,
                    key,
                    original_call,
                    error,
                    &mut control,
                )?;
            }
            Ok(())
        })();
        self.finish_native_publication(
            &caller,
            |windows| same_route(windows, &caller, &route),
            result,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Keep original call and IPC message correlations distinct through the same refusal owner."
    )]
    fn publish_renderer_refusal(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
        caller: &PrimaryCallerContext,
        route: &RendererRoute,
        key: NativeRouteKey,
        original_call: u32,
        error: CallError,
        control: &mut Option<keld_wv::renderer_bridge::NativeControlReservation>,
    ) -> Result<(), HostAppError> {
        let retained = error
            .code
            .capacity()
            .checked_add(error.message.capacity())
            .and_then(|bytes| bytes.checked_add(1024))
            .ok_or_else(|| app_detail("compat native refusal", "diagnostic storage overflow"))?;
        if !control
            .as_ref()
            .is_some_and(|control| control.try_extend(1, retained.max(1025)))
        {
            return Err(app_detail(
                "compat native refusal",
                "original CONTROL storage cannot retain diagnostic",
            ));
        }
        let mut error = Some(error);
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows
                .lock()
                .map_err(|_| app_detail("compat native refusal", "Registry lock poisoned"))?;
            if !same_route(&windows, caller, route) {
                return Ok(());
            }
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat native refusal", "ledger lock poisoned"))?;
            if state.ledger.preflight.as_ref().is_some_and(|pending| {
                pending.call == original_call
                    && same_caller(&pending.caller, caller)
                    && pending.document.same_binding(&route.document)
            }) {
                state.ledger.preflight = None;
            }
            let error = error
                .take()
                .ok_or_else(|| app_detail("compat native refusal", "diagnostic disappeared"))?;
            state.send(RendererBridgeOutcome::SendResult {
                webview: WebviewId(key.webview),
                navigation: u64::from(key.navigation),
                request: key.message,
                disposition: WvTerminalDisposition::Refused {
                    code: error.code,
                    message: error.message,
                },
                control: control.take(),
            })
        })
    }
}

#[cfg(all(test, debug_assertions))]
mod tests;
