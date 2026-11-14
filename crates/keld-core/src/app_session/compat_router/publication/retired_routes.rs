//! The existing synchronous retirement source owns one scalar route continuation.
//! Native posting crosses released short fences; no payload-bearing replay queue.
use super::{
    Arc, HostAppError, Instant, PrimaryCallerContext, PrimaryRouterHandle, RendererBridgeOutcome,
    WindowOwner, WindowPublicationTurn, app_detail,
};

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "One scalar cursor and original deadline retain the retired route across released-fence capacity waits."
    )]
    pub(in crate::app_session) fn publish_retired_compat_routes(
        &self,
        turn: &WindowPublicationTurn<'_>,
        caller: &PrimaryCallerContext,
        deadline: Instant,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        let writer = self.primary_writer_owner()?;
        let mut after = 0;
        loop {
            let route = {
                let _transition = self.shutdown.transition_guard();
                let current = self.current.lock().map_err(|_| {
                    app_detail("compat retired route cursor", "current lock poisoned")
                })?;
                if current.as_ref().is_some_and(|active| {
                    active.identity == caller.identity
                        && active.attempt == caller.attempt
                        && Arc::ptr_eq(&active.reader_stop, &caller.reader_stop)
                }) {
                    return Ok(());
                }
                let Some(windows) = &self.windows else {
                    return Ok(());
                };
                let session = windows.lock().map_err(|_| {
                    app_detail("compat retired route cursor", "Registry lock poisoned")
                })?;
                if !turn.owns(windows, &session) {
                    return Err(app_detail(
                        "compat retired route cursor",
                        "foreign source turn",
                    ));
                }
                session.next_retired_renderer_route_after(caller, after)
            };
            let Some(route) = route else {
                return Ok(());
            };
            after = route.window.id.0;
            let Some(retirement) = self.renderer_route_retirement(caller, &route) else {
                continue;
            };
            let mut retirement = Some(retirement);
            loop {
                if Instant::now() >= deadline {
                    return Err(app_detail(
                        "compat retired route storage",
                        "original retirement deadline expired",
                    ));
                }
                writer
                    .register_native_control_interest()
                    .map_err(|error| error.into_host_error("compat retired CONTROL interest"))?;
                let mut control = retirement.as_ref().and_then(|permit| {
                    permit.try_reserve_route_unavailable(&route.document, 1, 1025)
                });
                let retained;
                let mut sent = false;
                {
                    let _transition = self.shutdown.transition_guard();
                    let _current = self.current.lock().map_err(|_| {
                        app_detail("compat retired route admission", "current lock poisoned")
                    })?;
                    let Some(windows) = &self.windows else {
                        return Ok(());
                    };
                    let session = windows.lock().map_err(|_| {
                        app_detail("compat retired route admission", "Registry lock poisoned")
                    })?;
                    if !turn.owns(windows, &session) {
                        return Err(app_detail(
                            "compat retired route admission",
                            "foreign source turn",
                        ));
                    }
                    retained = session.retains_retiring_renderer_route(
                        WindowOwner::new(caller.identity, caller.attempt),
                        &caller.reader_stop,
                        route.window,
                        &route.document,
                    );
                    if retained && control.is_some() {
                        if Instant::now() >= deadline {
                            return Err(app_detail(
                                "compat retired route admission",
                                "original retirement deadline expired",
                            ));
                        }
                        let state = compat.lock().map_err(|_| {
                            app_detail("compat retired route admission", "ledger lock poisoned")
                        })?;
                        let permit = retirement.take().ok_or_else(|| {
                            app_detail("compat retired route admission", "exact permit disappeared")
                        })?;
                        state.send(RendererBridgeOutcome::RouteState {
                            webview: route.document.stamp().view(),
                            navigation: u64::from(route.document.stamp().observed_generation()),
                            ready: false,
                            role: self.renderer_role_witness_for_window(caller, route.window),
                            retirement: Some(permit),
                            control: control.take(),
                        })?;
                        sent = true;
                    }
                }
                drop(control);
                if sent || !retained {
                    break;
                }
                writer
                    .wait_retired_native_control_progress(deadline)
                    .map_err(|error| error.into_host_error("compat retired CONTROL wait"))?;
            }
        }
    }
}
