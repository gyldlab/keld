//! Actual Subscribed follows physically completed bounded replay on the sole writer.
use super::routes::RowPublication;
use super::{
    Arc, ControlMessage, CorrelationId, HostAppError, Instant, LifecycleResponse,
    NativePublicationDisposition, NativeRouteKey, PrimaryCallerContext, PrimaryRouterHandle,
    PublicationResult, RendererBridgeRequest, SubscribeRefusal, WindowOwner, WindowPublicationTurn,
    WriterLink, app_detail, exact_document, same_route, with_primary_caller_locked,
};

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn subscribe_compat(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<bool, HostAppError> {
        let entry = keld_ipc::channel_table::COMPAT_CONTROL;
        if channel != entry.wire_id() {
            return Ok(false);
        }
        self.primary_writer_owner()?;
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let turn = self.claim_window_publication(deadline, Some(caller))?;
        let target = self.primary_writer_target(caller)?;
        let result = if let Some(target) = target {
            self.publish_compat_subscription(turn.as_ref(), caller, &target, correlation, deadline)
        } else {
            Ok(())
        };
        drop(turn);
        let result = self.finish_call_publication(result);
        let compat = self.service_compat_writer_progress();
        let dialog = self.service_dialog_writer_progress();
        crate::app_session::collapse_app_results([result, compat, dialog])?;
        Ok(true)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "One subscription source orders bounded replay, physical reply and readiness under the original deadline."
    )]
    fn publish_compat_subscription(
        &self,
        turn: Option<&WindowPublicationTurn<'_>>,
        caller: &PrimaryCallerContext,
        target: &Arc<WriterLink>,
        correlation: CorrelationId,
        deadline: Instant,
    ) -> PublicationResult<()> {
        let entry = keld_ipc::channel_table::COMPAT_CONTROL;
        let mut response = None;
        let mut publish_ready = false;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let reason = if !entry.class().carries_host_events()
                || !matches!(
                    entry.authority(),
                    keld_ipc::channel_table::Authority::RendererChannelGuardQuery
                )
                || self.compat.is_none()
            {
                Some(SubscribeRefusal::NotSubscribable)
            } else if !caller.identity.is_primary() {
                Some(SubscribeRefusal::RoleNotAdmitted)
            } else {
                None
            };
            response = Some(reason.map_or(LifecycleResponse::Subscribed, |reason| {
                LifecycleResponse::SubscribeRefused { reason }
            }));
            if reason.is_none() {
                let compat = self.compat.as_ref().ok_or_else(|| {
                    app_detail("compat Subscribe", "validated service disappeared")
                })?;
                let previous = {
                    let mut state = compat
                        .lock()
                        .map_err(|_| app_detail("compat Subscribe", "session lock poisoned"))?;
                    let previous = state.subscribed_to(caller);
                    state.subscribed = None;
                    previous
                };
                let rebound = if let Some(windows) = &self.windows {
                    windows
                        .lock()
                        .map_err(|_| app_detail("compat Subscribe", "registry lock poisoned"))?
                        .rebind_recovered_renderer_documents(WindowOwner::new(
                            caller.identity,
                            caller.attempt,
                        ))
                } else {
                    false
                };
                publish_ready = !previous || rebound;
            }
            Ok(())
        })?;
        let Some(response) = response else {
            return Ok(());
        };
        let accepted = matches!(response, LifecycleResponse::Subscribed);
        if accepted && let Some(turn) = turn {
            let mut after = 0;
            while let Some(route) = self.next_compat_route(caller, after)? {
                match self.publish_compat_document_bound(turn, caller, target, &route, deadline)? {
                    RowPublication::RoleGone => return Ok(()),
                    RowPublication::Written | RowPublication::Stale => after = route.window.id.0,
                }
            }
        }
        let frame = self.prepare_primary_control(
            caller,
            Arc::clone(target),
            ControlMessage::LifecycleReply {
                correlation,
                response,
            },
            deadline,
            "compat Subscribe reply preparation",
        )?;
        let Some(receipt) = self.admit_primary_frame(caller, frame)? else {
            return Ok(());
        };
        self.observe_publication(
            target,
            receipt,
            deadline,
            "compat Subscribe reply completion",
        )?;
        let mut committed = false;
        if accepted {
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
                if let Some(compat) = &self.compat {
                    compat
                        .lock()
                        .map_err(|_| {
                            app_detail("compat Subscribe commit", "session lock poisoned")
                        })?
                        .subscribed = Some(caller.clone());
                    committed = true;
                }
                Ok(())
            })?;
        }
        if committed
            && publish_ready
            && let Some(turn) = turn
        {
            let mut after = 0;
            while let Some(route) = self.next_compat_route(caller, after)? {
                self.publish_compat_route_ready_after_bound(turn, caller, &route, deadline)?;
                after = route.window.id.0;
            }
        }
        Ok(())
    }

    pub(super) fn note_renderer_registered(
        &self,
        request: &RendererBridgeRequest,
        key: NativeRouteKey,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let Some(document) = request
            .document_lease()
            .filter(|document| document.is_current() && exact_document(document, key))
        else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let target = self.current_primary_writer_target()?;
        let turn = self.claim_window_publication(deadline, None);
        let Some(windows) = &self.windows else {
            return Ok(NativePublicationDisposition::Handled);
        };
        if windows
            .lock()
            .map_err(|_| app_detail("compat native registration", "registry lock poisoned"))?
            .note_renderer_registered(request.webview(), document.clone())
            .is_err()
        {
            return Ok(NativePublicationDisposition::Handled);
        }
        let Some(target) = target else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let caller = target.caller().clone();
        let mut route = None;
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |_| {
            let windows = windows
                .lock()
                .map_err(|_| app_detail("compat native registration", "registry lock poisoned"))?;
            let Some(compat) = &self.compat else {
                return Ok(());
            };
            if !compat
                .lock()
                .map_err(|_| app_detail("compat native registration", "session lock poisoned"))?
                .subscribed_to(&caller)
            {
                return Ok(());
            }
            route = windows
                .renderer_route_for_caller(&caller, request.webview(), Some(key.navigation))
                .ok()
                .filter(|route| route.document.same_binding(document));
            Ok(())
        })?;
        let Some(route) = route else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let turn = match turn {
            Ok(Some(turn)) => turn,
            Ok(None) => return Ok(NativePublicationDisposition::Handled),
            Err(error) => {
                return self.finish_native_publication(
                    &caller,
                    |windows| same_route(windows, &caller, &route),
                    Err(error.into()),
                );
            }
        };
        let result = (|| {
            if matches!(
                self.publish_compat_document_bound(&turn, &caller, &target, &route, deadline)?,
                RowPublication::Written
            ) {
                self.publish_compat_route_ready_after_bound(&turn, &caller, &route, deadline)?;
            }
            Ok(())
        })();
        drop(turn);
        let disposition = self.finish_native_publication(
            &caller,
            |windows| same_route(windows, &caller, &route),
            result,
        )?;
        if matches!(disposition, NativePublicationDisposition::Handled) {
            crate::app_session::collapse_app_results([
                self.service_compat_writer_progress(),
                self.service_dialog_writer_progress(),
            ])?;
        }
        Ok(disposition)
    }
}
