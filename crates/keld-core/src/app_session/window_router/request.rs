//! One Window request policy and typed result for both publication entry points.

use super::super::window_registry::Teardown;
use super::{
    ActivePrimaryGeneration, AppWindowCommand, CloseOutcome, CorrelationId, CreateOutcome,
    HostAppError, IpcError, NativeSetter, Ordering, PrimaryRouterHandle, WindowError,
    WindowRequest, WindowResponse, WindowSession, app_detail, app_ipc, decode, owner,
    report_abandoned,
};

pub(super) enum WindowRequestInput {
    Request(WindowRequest),
    Refused(WindowRefusal),
    DuplicateCorrelation,
}

pub(super) enum WindowRefusal {
    Registry(WindowError),
    Fixed {
        code: &'static str,
        message: &'static str,
    },
}

impl WindowRefusal {
    const fn fixed(code: &'static str, message: &'static str) -> Self {
        Self::Fixed { code, message }
    }

    pub(super) fn into_parts(self) -> (&'static str, String) {
        match self {
            Self::Registry(error) => (error.code(), error.to_string()),
            Self::Fixed { code, message } => (code, message.to_owned()),
        }
    }
}

pub(super) enum WindowRequestOutcome {
    Deferred,
    Reply(WindowResponse),
    Refused(WindowRefusal),
    Close(CloseOutcome),
    Teardown(Teardown),
}

impl PrimaryRouterHandle {
    pub(super) fn prepare_window_request_input_locked(
        &self,
        session: Option<&WindowSession>,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<WindowRequestInput, HostAppError> {
        if let Some(refusal) = self.check_window_request_locked(session, active, correlation) {
            return Ok(refusal);
        }
        decode(payload)
            .map(WindowRequestInput::Request)
            .map_err(|source| app_ipc("window request decode", &source))
    }

    pub(super) fn check_window_request_locked(
        &self,
        session: Option<&WindowSession>,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
    ) -> Option<WindowRequestInput> {
        let Some(session) = session.filter(|session| session.subscribed == Some(owner(active)))
        else {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-045",
                "KELD-CORE-045: window link is not subscribed. Complete the accepted window subscription and replay before issuing window calls.",
            )));
        };
        if !self.window_ready.load(Ordering::Acquire) {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-040",
                "KELD-CORE-040: registry is not ready. Wait for host Ready and accepted subscription/replay.",
            )));
        }
        if !session.native_supported {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-042",
                "KELD-CORE-042: native window calls are unsupported by this platform adapter. Use a qualified window backend.",
            )));
        }
        if session
            .registry
            .has_pending_call(owner(active), correlation)
        {
            return Some(WindowRequestInput::DuplicateCorrelation);
        }
        None
    }

    pub(super) fn prepare_window_request_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: WindowRequest,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        use WindowRequestOutcome::{Close, Deferred, Refused, Reply};
        let caller = owner(active);
        let outcome = match request {
            WindowRequest::Create {
                width,
                height,
                x,
                y,
            } => {
                match session
                    .registry
                    .create(caller, correlation, width, height, x, y)
                {
                    Ok(CreateOutcome::Build {
                        build,
                        width,
                        height,
                        x,
                        y,
                        ..
                    }) => {
                        self.send_window_request_command(
                            AppWindowCommand::CreateWindow {
                                build,
                                width,
                                height,
                                x,
                                y,
                                #[cfg(windows)]
                                original_source: None,
                                #[cfg(windows)]
                                preload_image: None,
                                #[cfg(windows)]
                                original_role: None,
                                #[cfg(windows)]
                                original_application_name_for_user_agent: None,
                                #[cfg(windows)]
                                original_create_start_by: None,
                            },
                            "window native build",
                        )?;
                        Deferred
                    }
                    Ok(CreateOutcome::Adopted(window)) => Reply(WindowResponse::Adopted { window }),
                    Err(error) => Refused(WindowRefusal::Registry(error)),
                }
            }
            WindowRequest::SetMaximized { window, maximized } => self
                .prepare_window_setter(session.registry.set_maximized(caller, window, maximized))?,
            WindowRequest::SetFullScreen {
                window,
                full_screen,
            } => self.prepare_window_setter(session.registry.set_full_screen(
                caller,
                window,
                full_screen,
            ))?,
            WindowRequest::RequestClose { window } => session
                .registry
                .request_close(caller, window)
                .map_or_else(registry_refusal, Close),
            WindowRequest::CloseReply {
                window,
                close_seq,
                verdict,
            } => session
                .registry
                .close_reply(caller, window, close_seq, verdict)
                .map_or_else(registry_refusal, Close),
            WindowRequest::CreateWithPreload { .. }
            | WindowRequest::LoadUrl { .. }
            | WindowRequest::DestroyOriginalDocument { .. } => Refused(WindowRefusal::fixed(
                "KELD-CORE-042",
                "KELD-CORE-042: Original-window admission is unavailable. Use a qualified native adapter.",
            )),
            WindowRequest::Destroy { window } => {
                match session
                    .registry
                    .destroy(caller, window, correlation, report_abandoned)
                {
                    Ok(CloseOutcome::Teardown(teardown)) => {
                        WindowRequestOutcome::Teardown(teardown)
                    }
                    Ok(CloseOutcome::AlreadyClosing) => Deferred,
                    Ok(_) => {
                        return Err(app_detail(
                            "window Destroy",
                            "registry returned an invalid destruction effect",
                        ));
                    }
                    Err(error) => registry_refusal(error),
                }
            }
        };
        Ok(outcome)
    }

    fn prepare_window_setter(
        &self,
        outcome: Result<NativeSetter, WindowError>,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        let command = match outcome {
            Ok(NativeSetter::Maximized { view, maximized }) => {
                AppWindowCommand::SetMaximized { view, maximized }
            }
            Ok(NativeSetter::FullScreen { view, full_screen }) => {
                AppWindowCommand::SetFullScreen { view, full_screen }
            }
            Err(error) => return Ok(registry_refusal(error)),
        };
        self.send_window_request_command(command, "window setter")?;
        Ok(WindowRequestOutcome::Reply(WindowResponse::Accepted))
    }

    fn send_window_request_command(
        &self,
        command: AppWindowCommand,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        self.window_commands
            .send(command)
            .map_err(|_| app_detail(phase, "UI command owner is unavailable"))
    }
}

fn registry_refusal(error: WindowError) -> WindowRequestOutcome {
    WindowRequestOutcome::Refused(WindowRefusal::Registry(error))
}

pub(super) fn duplicate_correlation() -> HostAppError {
    app_ipc(
        "window correlation admission",
        &IpcError::Protocol {
            detail: "window CALL reuses an already-pending correlation",
        },
    )
}
