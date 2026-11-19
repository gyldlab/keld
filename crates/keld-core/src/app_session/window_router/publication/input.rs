//! The primary reader keeps one synchronous request source through publication.

use super::super::original::{OriginalWindowCreateRefusal, PreparedOriginalWindowCreate};
use super::super::request::{
    CloseReplyPublication, WindowRefusal, WindowRequestInput, WindowRequestOutcome,
    duplicate_correlation, prepare_close_reply,
};
use super::super::{
    CorrelationId, HostAppError, PrimaryCallerContext, PrimaryRouterHandle, WindowRequest,
    WindowResponse, app_detail, with_primary_caller_locked,
};
use super::{PublicationResult, WindowEmitter, with_cleanup};
use std::time::{Duration, Instant};

const ORIGINAL_CREATE_PUBLICATION_LIMIT: Duration = Duration::from_secs(55);

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn dispatch_window_ordered(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        self.primary_writer_owner()?;
        let started = Instant::now();
        let deadline = started + keld_ipc::APP_LINK_IO_DEADLINE;
        let turn = self.claim_window_publication(deadline, Some(caller))?;
        let result = (|| {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            let mut input = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
                let active = current
                    .as_ref()
                    .ok_or_else(|| app_detail("Window request", "validated role disappeared"))?;
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
            let mut emitter = WindowEmitter {
                router: self,
                turn,
                target: &target,
                deadline,
            };
            let request = match input {
                WindowRequestInput::Refused(refusal) => {
                    return emitter.refusal(correlation, refusal, false, None);
                }
                WindowRequestInput::Request(request) => request,
                WindowRequestInput::DuplicateCorrelation => {
                    return Err(duplicate_correlation().into());
                }
            };
            let original = matches!(&request, WindowRequest::CreateWithPreload { .. });
            let mut outcome = None;
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
                let active = current.as_ref().ok_or_else(|| {
                    app_detail("Window request admission", "validated role disappeared")
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
                        "foreign Window publication turn",
                    ));
                }
                let prepared =
                    match self.check_window_request_locked(Some(&session), active, correlation) {
                        Some(WindowRequestInput::Refused(refusal)) => {
                            WindowRequestOutcome::Refused(refusal)
                        }
                        Some(WindowRequestInput::DuplicateCorrelation) => {
                            return Err(duplicate_correlation());
                        }
                        Some(WindowRequestInput::Request(_)) => {
                            return Err(app_detail(
                                "Window request admission",
                                "preflight returned a second request",
                            ));
                        }
                        None if original => {
                            match self.prepare_original_window_create_locked(
                                &mut session,
                                caller,
                                correlation,
                                request,
                                started,
                            ) {
                                Ok(PreparedOriginalWindowCreate::Build(command)) => {
                                    self.send_window_request_command(
                                        command,
                                        "window native build",
                                    )?;
                                    WindowRequestOutcome::Deferred
                                }
                                Ok(PreparedOriginalWindowCreate::Adopted(window)) => {
                                    WindowRequestOutcome::Reply(WindowResponse::Adopted { window })
                                }
                                Err(error) => {
                                    WindowRequestOutcome::Refused(original_refusal(error)?)
                                }
                            }
                        }
                        None => self.prepare_window_request_locked(
                            &mut session,
                            active,
                            correlation,
                            request,
                        )?,
                    };
                if original
                    && matches!(
                        prepared,
                        WindowRequestOutcome::Reply(_) | WindowRequestOutcome::Refused(_)
                    )
                {
                    emitter.deadline = (started + ORIGINAL_CREATE_PUBLICATION_LIMIT)
                        .min(Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE);
                }
                outcome = Some(prepared);
                Ok(())
            })?;
            if let Some(outcome) = outcome {
                emitter.request(correlation, outcome)?;
            }
            Ok(())
        })();
        drop(turn);
        self.finish_call_publication(result)
    }
}

fn original_refusal(error: OriginalWindowCreateRefusal) -> Result<WindowRefusal, HostAppError> {
    Ok(match error {
        OriginalWindowCreateRefusal::WrongRequest => {
            return Err(app_detail(
                "original Window request",
                "unexpected original Create dispatch",
            ));
        }
        OriginalWindowCreateRefusal::Registry(error) => WindowRefusal::Registry(error),
        OriginalWindowCreateRefusal::Preload(error) => WindowRefusal::OriginalPreload(error),
        OriginalWindowCreateRefusal::Deadline => WindowRefusal::fixed(
            "KELD-CORE-041",
            "KELD-CORE-041: verified original source preparation exceeded the Windows Create admission cutoff; no native window was admitted. Check stage file access and relaunch.",
        ),
        OriginalWindowCreateRefusal::Unavailable => WindowRefusal::fixed(
            "KELD-CORE-042",
            "KELD-CORE-042: Original-window admission is unavailable. Use a qualified native adapter.",
        ),
    })
}

impl WindowEmitter<'_, '_> {
    fn request(
        &self,
        correlation: CorrelationId,
        outcome: WindowRequestOutcome,
    ) -> PublicationResult<()> {
        match outcome {
            WindowRequestOutcome::Deferred => Ok(()),
            WindowRequestOutcome::Reply(response) => {
                let source = match response {
                    WindowResponse::Adopted { window } => Some(window),
                    _ => None,
                };
                self.reply(correlation, response, source)
            }
            WindowRequestOutcome::Refused(refusal) => {
                self.refusal(correlation, refusal, false, None)
            }
            WindowRequestOutcome::Teardown(teardown) => self
                .router
                .cleanup_window_action(teardown, "Window teardown")
                .map_err(Into::into),
            WindowRequestOutcome::Close(close) => match prepare_close_reply(close)? {
                CloseReplyPublication::Reply { response, fact } => {
                    if let Some(fact) = fact {
                        self.fact(&fact)?;
                    }
                    self.reply(correlation, response, None)
                }
                CloseReplyPublication::Teardown(teardown) => {
                    let reply = self.reply(
                        correlation,
                        WindowResponse::ReplyApplied,
                        Some(teardown.window),
                    );
                    let cleanup = self
                        .router
                        .cleanup_window_action(teardown, "Window teardown");
                    with_cleanup(reply, cleanup)
                }
            },
        }
    }
}
