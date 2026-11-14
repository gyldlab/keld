//! The existing worker's one current request/call/result; no payload queue.
use super::{
    APP_LINK_READER_POLL, AtomicBool, DispatchFailure, HostAppError, Ordering, PrimaryEchoReply,
    PrimaryRouterHandle, Receiver, RecvTimeoutError, RendererBridgeOutcome, RendererBridgeRequest,
    Sender, app_detail, compat_router, mpsc, numeric_echo, retained, window_router,
};
use keld_wv::renderer_bridge::NativeNumericResultReservation;

pub(super) struct Input {
    requests: Receiver<RendererBridgeRequest>,
    outcomes: Sender<RendererBridgeOutcome>,
    router: PrimaryRouterHandle,
    // Drop body-bearing carriers before any separate result reservation.
    request: Option<RendererBridgeRequest>,
    call: Option<numeric_echo::Call>,
    outcome: Option<RendererBridgeOutcome>,
    numeric_storage: Option<NativeNumericResultReservation>,
}

impl Input {
    pub(super) fn new(
        requests: Receiver<RendererBridgeRequest>,
        outcomes: Sender<RendererBridgeOutcome>,
        router: PrimaryRouterHandle,
    ) -> Self {
        Self {
            requests,
            outcomes,
            router,
            request: None,
            call: None,
            outcome: None,
            numeric_storage: None,
        }
    }

    fn idle(&self) -> bool {
        self.request.is_none()
            && self.call.is_none()
            && self.outcome.is_none()
            && self.numeric_storage.is_none()
    }

    pub(super) fn check_empty(&mut self) -> Result<(), HostAppError> {
        if !self.idle() {
            return Err(app_detail(
                "renderer dispatch drain",
                "current native input or outcome remains",
            ));
        }
        match self.requests.try_recv() {
            Ok(request) => {
                self.request = Some(request);
                Err(app_detail(
                    "renderer dispatch drain",
                    "original request channel still owns input",
                ))
            }
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => Ok(()),
        }
    }

    fn forward(&mut self) -> Result<(), HostAppError> {
        retained::send(&mut self.outcome, &self.outcomes).map_err(|()| {
            app_detail(
                "renderer outcome",
                "original native receiver ended; actual outcome retained",
            )
        })
    }

    pub(super) fn run(&mut self, stop: &AtomicBool) -> Result<(), DispatchFailure> {
        loop {
            if stop.load(Ordering::Acquire) {
                return if self.idle() {
                    Ok(())
                } else {
                    Err(
                        app_detail("renderer dispatch stop", "current native custody remains")
                            .into(),
                    )
                };
            }
            // No shared-state/canonical lock is held across this original wait.
            self.request = match self.requests.recv_timeout(APP_LINK_READER_POLL) {
                Ok(request) => Some(request),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            };
            // Actual dequeue freed the existing bounded request-channel position.
            self.outcome = Some(RendererBridgeOutcome::ControlCapacity);
            self.forward()?;
            match self
                .router
                .dispatch_renderer_request_retained(&mut self.request)
            {
                Ok(compat_router::RendererRequestDisposition::Invoke) => {}
                Ok(compat_router::RendererRequestDisposition::Complete(
                    window_router::NativePublicationDisposition::Handled,
                )) => {
                    if self.request.is_some() {
                        return Err(app_detail(
                            "renderer dispatch",
                            "completed input is still attached",
                        )
                        .into());
                    }
                    continue;
                }
                Ok(compat_router::RendererRequestDisposition::Complete(
                    window_router::NativePublicationDisposition::FatalQueued(error),
                )) => return Err(DispatchFailure::FatalQueued(error)),
                Ok(compat_router::RendererRequestDisposition::RetainedFailure {
                    cause,
                    disposition,
                }) => return Err(DispatchFailure::from_source(cause, disposition)),
                // No generic error or ambiguous old source grants authority to
                // terminate whichever caller is current now.
                Err(error) => return Err(DispatchFailure::RetiredSource(error)),
            }
            self.numeric(stop)?;
        }
    }

    fn numeric(&mut self, stop: &AtomicBool) -> Result<(), HostAppError> {
        let request = self
            .request
            .as_mut()
            .ok_or_else(|| app_detail("renderer numeric call", "original request disappeared"))?;
        self.numeric_storage = request.take_numeric_result();
        let storage = self.numeric_storage.as_ref().ok_or_else(|| {
            app_detail(
                "renderer numeric call",
                "original result reservation is absent",
            )
        })?;
        let lease = request
            .document_lease()
            .ok_or_else(|| app_detail("renderer numeric call", "original lease is absent"))?;
        if !storage.matches_request(
            request.webview(),
            request.navigation(),
            request.request(),
            lease,
        ) {
            return Err(app_detail(
                "renderer numeric call",
                "result reservation belongs to another request",
            ));
        }
        numeric_echo::accounting::require_prepaid(storage)?;
        let admission =
            self.router
                .begin_numeric_echo(request, &mut self.numeric_storage, &mut self.call);
        if self.numeric_storage.is_some() {
            // No token transfer means no pending record owns this operation.
            // Drop the unadmitted channel before forming/forwarding native bytes.
            drop(self.call.take());
            if admission.is_ok() {
                return Err(app_detail(
                    "renderer numeric call",
                    "admission did not retain its token",
                ));
            }
            self.fixed_error("application call is unavailable or already pending")?;
            drop(self.request.take());
            return self.forward();
        }
        if self.call.is_none() {
            return Err(app_detail(
                "renderer numeric call",
                "admitted call receiver is absent",
            ));
        }
        // An admitted physical failure has already used its exact writer fact.
        // Its original pending record/receiver remains, including a cancellation
        // while the old reader owns a partial body. Never fabricate B's result.
        let _ = admission;
        loop {
            self.router.service_numeric_echo_progress()?;
            let call = self.call.as_mut().ok_or_else(|| {
                app_detail("renderer numeric reply", "actual call receiver disappeared")
            })?;
            let request = self.request.as_ref().ok_or_else(|| {
                app_detail("renderer numeric reply", "actual request disappeared")
            })?;
            if self
                .router
                .take_numeric_echo_outcome(call, request, &mut self.outcome)?
            {
                // Both std channel endpoints disappear before native encoding.
                drop(self.call.take());
                drop(self.request.take());
                return self.forward();
            }
            if stop.load(Ordering::Acquire) {
                return Err(app_detail(
                    "renderer numeric stop",
                    "actual call/result custody remains",
                ));
            }
            if call.notice.is_none() {
                call.notice = match call.inner.reply.recv_timeout(APP_LINK_READER_POLL) {
                    Ok(notice) => Some(notice),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(app_detail(
                            "renderer numeric reply",
                            "original notice channel ended",
                        ));
                    }
                };
            }
            if matches!(call.notice, Some(Ok(PrimaryEchoReply::NumericReady))) {
                drop(call.notice.take());
            } else {
                // Retain even an impossible actual foreign delivery in the same
                // call owner; it is never treated as numeric storage authority.
                return Err(app_detail(
                    "renderer numeric reply",
                    "unexpected notice from original call",
                ));
            }
        }
    }

    fn fixed_error(&mut self, detail: &'static str) -> Result<(), HostAppError> {
        if self.outcome.is_some() {
            return Err(app_detail(
                "renderer numeric error",
                "outcome slot is occupied",
            ));
        }
        let storage = self.numeric_storage.as_ref().ok_or_else(|| {
            app_detail(
                "renderer numeric error",
                "original result storage is absent",
            )
        })?;
        let code = "KELD-WV-011";
        if !numeric_echo::accounting::covers(storage, code.len() + detail.len()) {
            return Err(app_detail(
                "renderer numeric error",
                "fixed terminal is not prepaid",
            ));
        }
        let code = String::from(code);
        let detail = String::from(detail);
        let request = self
            .request
            .as_ref()
            .ok_or_else(|| app_detail("renderer numeric error", "original request is absent"))?;
        // Only infallible moves from the already attached token slot into the
        // already attached outcome follow all validation/allocation.
        self.outcome = Some(RendererBridgeOutcome::Error {
            webview: request.webview(),
            navigation: request.navigation(),
            request: request.request(),
            code,
            detail,
            control: None,
            numeric_result: self.numeric_storage.take(),
        });
        Ok(())
    }
}
