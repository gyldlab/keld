//! Bounded native CONTROL storage uses the same ledger and physical release wake.
use super::{
    Arc, HostAppError, Instant, MAX_CONTROL_BYTES, PrimaryCallerContext, PrimaryRouterHandle,
    PublicationResult, RendererBridgeOutcome, RendererRoute, WindowPublicationTurn, WriterLink,
    app_detail, routes, with_primary_caller_locked,
};
use keld_wv::renderer_bridge::NativeControlReservation;

impl PrimaryRouterHandle {
    /// Storage only. The Registry still owns the actual close decision and the
    /// final native post still uses the exact old-route permit/lease.
    pub(in crate::app_session) fn reserve_native_route_control(
        &self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
        deadline: Instant,
    ) -> Result<Option<NativeControlReservation>, HostAppError> {
        self.reserve_native_source_control(caller, window, deadline, 1025)
    }

    pub(super) fn reserve_native_send_control(
        &self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
        deadline: Instant,
    ) -> Result<Option<NativeControlReservation>, HostAppError> {
        self.reserve_native_source_control(caller, window, deadline, MAX_CONTROL_BYTES + 1024)
    }

    // Closed callers above select only fixed route metadata or bounded refusal
    // diagnostics. This storage helper supplies no effect or retired authority.
    fn reserve_native_source_control(
        &self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
        deadline: Instant,
        accounted: usize,
    ) -> Result<Option<NativeControlReservation>, HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(None);
        };
        let writer = self.primary_writer_owner()?;
        loop {
            if Instant::now() >= deadline {
                return Err(app_detail(
                    "native route CONTROL",
                    "original source deadline expired",
                ));
            }
            writer
                .register_native_control_interest()
                .map_err(|error| error.into_host_error("native route CONTROL interest"))?;
            let mut entered = false;
            let mut control = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
                let Some(windows) = &self.windows else {
                    return Ok(());
                };
                let windows = windows
                    .lock()
                    .map_err(|_| app_detail("native route CONTROL", "Registry lock poisoned"))?;
                if !windows.native_offers_open_for(caller, window) {
                    return Ok(());
                }
                entered = true;
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("native route CONTROL", "ledger lock poisoned"))?;
                if state.ledger.control_progress.is_none() {
                    return Err(app_detail(
                        "native route CONTROL",
                        "same physical progress owner is unavailable",
                    ));
                }
                control = state.reserve_native_control_for_window(
                    caller,
                    window,
                    1,
                    accounted,
                    Arc::clone(compat),
                );
                Ok(())
            })?;
            if !entered || control.is_some() {
                return Ok(control);
            }
            writer
                .wait_native_control_progress(caller, deadline)
                .map_err(|error| error.into_host_error("native route CONTROL wait"))?;
        }
    }

    /// Called only at the Registry's exact admitted before-clear close cut.
    /// The supplied slot is consumed once; this method never reserves or waits.
    pub(in crate::app_session) fn compat_route_retired_reserved_locked(
        &self,
        caller: &PrimaryCallerContext,
        route: &RendererRoute,
        control: &mut Option<NativeControlReservation>,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        if !route.document.is_current() {
            return Ok(());
        }
        let retirement = self
            .renderer_route_retirement(caller, route)
            .ok_or_else(|| {
                app_detail(
                    "native route retirement",
                    "exact retained route permit is unavailable",
                )
            })?;
        let state = compat
            .lock()
            .map_err(|_| app_detail("native route retirement", "ledger lock poisoned"))?;
        let control = control.take().ok_or_else(|| {
            app_detail(
                "native route retirement",
                "existing CONTROL slot was not reserved",
            )
        })?;
        state.send(RendererBridgeOutcome::RouteState {
            webview: route.document.stamp().view(),
            navigation: u64::from(route.document.stamp().observed_generation()),
            ready: false,
            role: self.renderer_role_witness_for_window(caller, route.window),
            retirement: Some(retirement),
            control: Some(control),
        })
    }

    /// Window Committed already physically published its URL under this same
    /// turn. Reuse the sole DocumentBound/readiness producer without raw writes.
    pub(in crate::app_session) fn publish_window_compat_route(
        &self,
        turn: &WindowPublicationTurn<'_>,
        caller: &PrimaryCallerContext,
        target: &Arc<WriterLink>,
        window: keld_ipc::window::WindowRef,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
        deadline: Instant,
    ) -> PublicationResult<()> {
        let mut captured = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows
                .lock()
                .map_err(|_| app_detail("Window Compat binding", "Registry lock poisoned"))?;
            let Some(compat) = &self.compat else {
                return Ok(());
            };
            if !compat
                .lock()
                .map_err(|_| app_detail("Window Compat binding", "ledger lock poisoned"))?
                .subscribed_to(caller)
            {
                return Ok(());
            }
            captured = windows
                .renderer_route_for_caller(
                    caller,
                    document.stamp().view(),
                    Some(document.stamp().observed_generation()),
                )
                .ok()
                .filter(|route| route.window == window && route.document.same_binding(document));
            Ok(())
        })?;
        if let Some(route) = captured
            && matches!(
                self.publish_compat_document_bound(turn, caller, target, &route, deadline)?,
                routes::RowPublication::Written
            )
        {
            self.publish_compat_route_ready_after_bound(turn, caller, &route, deadline)?;
        }
        Ok(())
    }
}
