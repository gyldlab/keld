//! The original reader's bounded request remains its source until exact admission.
use super::super::request::{WindowCloseControl, WindowRequestInput, duplicate_correlation};
use super::{
    CorrelationId, HostAppError, Instant, PrimaryCallerContext, PrimaryRouterHandle, WindowEmitter,
    WindowRequest, WindowRequestOutcome, app_detail, collapse_app_results,
    with_primary_caller_locked,
};
use keld_ipc::window::CloseVerdict;

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the original request, close reservation, final Registry admission, and publication cleanup in their existing synchronous order."
    )]
    pub(in crate::app_session) fn dispatch_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        self.primary_writer_owner()?; // Windows never falls back to a raw writer.
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let turn = self.claim_window_publication(deadline, Some(caller))?;
        let result = (|| {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            let mut input = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
                let active = current
                    .as_ref()
                    .ok_or_else(|| app_detail("Window request", "validated caller disappeared"))?;
                let session = self
                    .windows
                    .as_ref()
                    .map(|windows| {
                        windows
                            .lock()
                            .map_err(|_| app_detail("Window request", "Registry lock poisoned"))
                    })
                    .transpose()?;
                input = Some(self.prepare_window_request_input_locked(
                    session.as_deref(),
                    active,
                    correlation,
                    payload,
                )?);
                Ok(())
            })?;
            let Some(input) = input else {
                return Ok(());
            };
            let Some(turn) = turn.as_ref() else {
                let WindowRequestInput::Refused(refusal) = input else {
                    return Err(app_detail(
                        "Window request",
                        "request admitted without a Registry",
                    )
                    .into());
                };
                let frame =
                    self.prepare_window_refusal(caller, &target, correlation, refusal, deadline)?;
                if let Some(receipt) = self.admit_primary_frame(caller, frame)? {
                    self.observe_publication(
                        &target,
                        receipt,
                        deadline,
                        "Window unavailable reply completion",
                    )?;
                }
                return Ok(());
            };
            let emitter = WindowEmitter {
                router: self,
                turn,
                target: &target,
                deadline,
            };
            let request = match input {
                WindowRequestInput::Request(request) => request,
                WindowRequestInput::Refused(refusal) => {
                    return emitter.refusal(correlation, refusal, false, None);
                }
                WindowRequestInput::DuplicateCorrelation => {
                    return Err(duplicate_correlation().into());
                }
            };
            // Reserve storage before the unchanged Registry decides whether a
            // close actually occurs. Wrong sequence/Veto/refusal consumes none.
            let close_window = match &request {
                WindowRequest::Destroy { window }
                | WindowRequest::CloseReply {
                    window,
                    verdict: CloseVerdict::Allow,
                    ..
                } => Some(*window),
                _ => None,
            };
            let control = match close_window {
                Some(window) => self.reserve_native_route_control(caller, window, deadline)?,
                None => None,
            };
            let mut close_control = WindowCloseControl::Reserved(control);
            let mut request = Some(request);
            let mut outcome = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
                let active = current.as_ref().ok_or_else(|| {
                    app_detail("Window request admission", "validated caller disappeared")
                })?;
                let windows = self.windows.as_ref().ok_or_else(|| {
                    app_detail("Window request admission", "Registry disappeared")
                })?;
                let mut session = windows.lock().map_err(|_| {
                    app_detail("Window request admission", "Registry lock poisoned")
                })?;
                if !turn.owns(windows, &session) {
                    return Err(app_detail(
                        "Window request admission",
                        "foreign source turn",
                    ));
                }
                if let Some(refusal) = self.check_window_request_locked(
                    Some(&session),
                    active,
                    correlation,
                    payload.len(),
                ) {
                    outcome = Some(match refusal {
                        WindowRequestInput::Refused(refusal) => {
                            WindowRequestOutcome::Refused(refusal)
                        }
                        WindowRequestInput::DuplicateCorrelation => {
                            return Err(duplicate_correlation());
                        }
                        WindowRequestInput::Request(_) => {
                            return Err(app_detail(
                                "Window request admission",
                                "preflight decoded a second body",
                            ));
                        }
                    });
                    return Ok(());
                }
                let request = request.take().ok_or_else(|| {
                    app_detail("Window request admission", "source request disappeared")
                })?;
                outcome = Some(self.prepare_window_request_locked(
                    &mut session,
                    active,
                    correlation,
                    request,
                    &mut close_control,
                )?);
                Ok(())
            })?;
            // An unused close reservation is returned only after the source cut.
            drop(close_control);
            drop(request);
            if let Some(outcome) = outcome {
                emitter.request(correlation, outcome)?;
            }
            Ok(())
        })();
        drop(turn);
        let result = self.finish_call_publication(result);
        self.finish_window_publication_service(result)
    }

    pub(in crate::app_session::window_router) fn finish_window_publication_service(
        &self,
        result: Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        // A pure capacity settlement may have consumed a completion while this
        // source owned the turn. Full service runs only after its Drop.
        let compat = self.service_compat_writer_progress();
        let dialog = self.service_dialog_writer_progress();
        collapse_app_results([result, compat, dialog])
    }
}
