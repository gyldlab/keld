//! One route at a time; the Registry and retained source stay the only document owners.
use super::{
    Arc, ControlMessage, HostAppError, Instant, NativeControlPublication, PrimaryCallerContext,
    PrimaryRouterHandle, PublicationResult, RendererBridgeOutcome, RendererRoute, WindowOwner,
    WindowPublicationTurn, WriterLink, admit_locked, app_detail, same_route,
    with_primary_caller_locked,
};

pub(super) enum RowPublication {
    Written,
    Stale,
    RoleGone,
}

impl PrimaryRouterHandle {
    pub(super) fn next_compat_route(
        &self,
        caller: &PrimaryCallerContext,
        after: u32,
    ) -> Result<Option<RendererRoute>, HostAppError> {
        let mut next = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows
                .lock()
                .map_err(|_| app_detail("compat route cursor", "registry lock poisoned"))?;
            let owner = WindowOwner::new(caller.identity, caller.attempt);
            let mut cursor = after;
            while let Some(route) = windows.next_registered_renderer_route_after(owner, cursor) {
                cursor = route.window.id.0;
                if same_route(&windows, caller, &route) {
                    next = Some(route);
                    break;
                }
            }
            Ok(())
        })?;
        Ok(next)
    }

    pub(super) fn publish_compat_document_bound(
        &self,
        turn: &WindowPublicationTurn<'_>,
        caller: &PrimaryCallerContext,
        target: &Arc<WriterLink>,
        route: &RendererRoute,
        deadline: Instant,
    ) -> PublicationResult<RowPublication> {
        let stamp = route.document.stamp();
        let frame = self.prepare_primary_control(
            caller,
            Arc::clone(target),
            ControlMessage::DocumentBound {
                webview: stamp.view().0,
                navigation: stamp.observed_generation(),
                window_id: route.window.id.0,
                window_incarnation: route.window.incarnation.0,
            },
            deadline,
            "compat DocumentBound preparation",
        )?;
        let mut frame = Some(frame);
        let mut receipt = None;
        let mut entered = false;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            entered = true;
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows.lock().map_err(|_| {
                app_detail("compat DocumentBound admission", "registry lock poisoned")
            })?;
            if !turn.owns(
                self.windows.as_ref().ok_or_else(|| {
                    app_detail("compat DocumentBound admission", "Registry disappeared")
                })?,
                &windows,
            ) {
                return Err(app_detail(
                    "compat DocumentBound admission",
                    "foreign publication turn",
                ));
            }
            if !same_route(&windows, caller, route) {
                return Ok(());
            }
            let active = current.as_ref().ok_or_else(|| {
                app_detail(
                    "compat DocumentBound admission",
                    "validated caller disappeared",
                )
            })?;
            receipt = Some(admit_locked(
                active,
                frame.take().ok_or_else(|| {
                    app_detail(
                        "compat DocumentBound admission",
                        "prepared frame disappeared",
                    )
                })?,
            )?);
            Ok(())
        })?;
        drop(frame);
        let Some(receipt) = receipt else {
            return Ok(if entered {
                RowPublication::Stale
            } else {
                RowPublication::RoleGone
            });
        };
        self.observe_publication(target, receipt, deadline, "compat DocumentBound completion")?;
        Ok(RowPublication::Written)
    }

    /// Caller already holds the canonical caller AND exact Registry source fence.
    /// A preceding `DocumentBound` was physically Written on the same writer.
    /// Full is a storage observation; caller releases all guards before waiting.
    pub(in crate::app_session) fn compat_route_ready_after_bound_locked(
        &self,
        caller: &PrimaryCallerContext,
        route: &RendererRoute,
    ) -> Result<NativeControlPublication, HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(NativeControlPublication::Stale);
        };
        let mut state = compat
            .lock()
            .map_err(|_| app_detail("compat route readiness", "session lock poisoned"))?;
        if !route.document.is_current()
            || !self.shutdown.is_running()
            || !state.subscribed_to(caller)
        {
            return Ok(NativeControlPublication::Stale);
        }
        if state.ledger.control_progress.is_none() {
            return Err(app_detail(
                "compat route readiness",
                "same CONTROL progress owner is unavailable",
            ));
        }
        let Some(control) = state.reserve_native_control_for_window(
            caller,
            route.window,
            1,
            1025,
            Arc::clone(compat),
        ) else {
            return Ok(NativeControlPublication::Full);
        };
        state.send(RendererBridgeOutcome::RouteState {
            webview: route.document.stamp().view(),
            navigation: u64::from(route.document.stamp().observed_generation()),
            ready: true,
            role: self.renderer_role_witness_for_window(caller, route.window),
            retirement: None,
            control: Some(control),
        })?;
        Ok(NativeControlPublication::Enqueued)
    }

    /// The shared turn and one stack route remain retained. Native posting can
    /// cross the released Core fence while this waits on existing storage.
    pub(in crate::app_session) fn publish_compat_route_ready_after_bound(
        &self,
        turn: &WindowPublicationTurn<'_>,
        caller: &PrimaryCallerContext,
        route: &RendererRoute,
        deadline: Instant,
    ) -> Result<(), HostAppError> {
        let writer = self.primary_writer_owner()?;
        loop {
            if Instant::now() >= deadline {
                return Err(app_detail(
                    "compat native CONTROL wait",
                    "original publication deadline expired",
                ));
            }
            writer
                .register_native_control_interest()
                .map_err(|error| error.into_host_error("compat native CONTROL interest"))?;
            let mut result = NativeControlPublication::Stale;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
                let Some(windows) = &self.windows else {
                    return Ok(());
                };
                let windows = windows
                    .lock()
                    .map_err(|_| app_detail("compat route readiness", "registry lock poisoned"))?;
                if !turn.owns(
                    self.windows.as_ref().ok_or_else(|| {
                        app_detail("compat route readiness", "Registry disappeared")
                    })?,
                    &windows,
                ) {
                    return Err(app_detail(
                        "compat route readiness",
                        "foreign publication turn",
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(app_detail(
                        "compat route readiness",
                        "original publication deadline expired",
                    ));
                }
                if same_route(&windows, caller, route) {
                    result = self.compat_route_ready_after_bound_locked(caller, route)?;
                }
                Ok(())
            })?;
            match result {
                NativeControlPublication::Enqueued | NativeControlPublication::Stale => {
                    return Ok(());
                }
                NativeControlPublication::Full => writer
                    .wait_native_control_progress(caller, deadline)
                    .map_err(|error| error.into_host_error("compat native CONTROL wait"))?,
            }
        }
    }
}
