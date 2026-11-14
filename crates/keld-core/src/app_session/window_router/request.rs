//! One request-policy match. Results retain existing typed source facts; each
//! platform's publication owner applies them in the established order.
use super::super::window_registry::{PendingWindowCall, Teardown};
#[cfg(any(target_os = "macos", windows))]
use super::OriginalLoadAdmission;
use super::{
    ActivePrimaryGeneration, AppWindowCommand, CloseOutcome, CorrelationId, CreateOutcome,
    HostAppError, IpcError, NativeSetter, Ordering, PrimaryRouterHandle, WindowBuildResources,
    WindowError, WindowRequest, WindowResponse, WindowSession, app_detail, app_ipc,
    append_app_cleanup, decode, owner, report_abandoned,
};
#[cfg(windows)]
use super::{BuildStage, PrimaryCallerContext};

mod closing;
mod input;
#[cfg(any(not(windows), test))]
mod legacy;
mod original;
#[cfg(windows)]
pub(super) use closing::WindowCloseControl;
pub(super) use input::{WindowRequestInput, duplicate_correlation};

pub(super) enum WindowRefusal {
    Registry(WindowError),
    Fixed {
        code: &'static str,
        message: &'static str,
    },
    #[cfg(any(target_os = "macos", windows))]
    OriginalUri(HostAppError),
    #[cfg(any(target_os = "macos", windows))]
    OriginalPreload(HostAppError),
    #[cfg(windows)]
    Native {
        code: &'static str,
        detail: &'static str,
    },
    #[cfg(windows)]
    Build(BuildStage),
}
impl WindowRefusal {
    pub(super) fn fixed(code: &'static str, message: &'static str) -> Self {
        Self::Fixed { code, message }
    }
    // Windows invokes this only AFTER reserving VALUE. Original diagnostic
    // objects move from their source; no formatted output is stored in a queue.
    pub(super) fn into_parts(self) -> (&'static str, String) {
        let (code, message) = match self {
            Self::Registry(error) => (error.code(), error.to_string()),
            Self::Fixed { code, message } => (code, message.to_owned()),
            #[cfg(any(target_os = "macos", windows))]
            Self::OriginalUri(error) => (
                "KELD-CORE-051",
                format!(
                    "KELD-CORE-051: {error}; prepare the verified original source and send its exact selected entry URI"
                ),
            ),
            #[cfg(any(target_os = "macos", windows))]
            Self::OriginalPreload(error) => (
                "KELD-CORE-051",
                format!(
                    "KELD-CORE-051: {error}; use the exact original preload and bounded captured arguments"
                ),
            ),
            #[cfg(windows)]
            Self::Native { code, detail } => (
                code,
                format!(
                    "{code}: {detail}; inspect the native diagnostic, correct it and issue a new explicit load"
                ),
            ),
            #[cfg(windows)]
            Self::Build(stage) => {
                let stage = match stage {
                    BuildStage::Window => "window",
                    BuildStage::Webview => "webview",
                };
                (
                    "KELD-CORE-041",
                    format!(
                        "KELD-CORE-041: native {stage} creation failed. Inspect the UI owner's typed error and correct the requested window resources."
                    ),
                )
            }
        };
        (code, message)
    }
}

pub(super) enum WindowRequestOutcome {
    Deferred,
    Reply(WindowResponse),
    Refused(WindowRefusal),
    Close(CloseOutcome),
    #[cfg(any(target_os = "macos", windows))]
    OriginalDestroy(super::super::window_registry::OriginalDocumentDestroyResolution),
    Closing(ClosingRequest),
}

/// This is the same admitted Closing action and existing bounded correlations.
/// It stays on the synchronous source stack until all preceding publications
/// have been attempted; failure does not cancel required physical cleanup.
pub(super) struct ClosingRequest {
    pub(super) availability: Result<(), HostAppError>,
    #[cfg(any(target_os = "macos", windows))]
    pub(super) scoped: super::super::window_registry::OriginalDocumentDestroyResolution,
    pub(super) interrupted: Option<PendingWindowCall>,
    pub(super) acknowledge: bool,
    pub(super) teardown: Teardown,
}

impl PrimaryRouterHandle {
    pub(super) fn prepare_window_request_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: WindowRequest,
        #[cfg(windows)] close_control: &mut WindowCloseControl,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        let caller = owner(active);
        match request {
            WindowRequest::Create {
                width,
                height,
                x,
                y,
            } => {
                #[cfg(not(windows))]
                let outcome = session
                    .registry
                    .create(caller, correlation, width, height, x, y);
                #[cfg(windows)]
                let outcome = session.registry.create_for_caller(
                    &PrimaryCallerContext {
                        identity: active.identity,
                        attempt: active.attempt,
                        reader_stop: std::sync::Arc::clone(&active.reader_stop),
                    },
                    correlation,
                    width,
                    height,
                    x,
                    y,
                );
                self.prepare_window_create_result(outcome, WindowBuildResources::default())
            }
            request @ WindowRequest::CreateWithPreload { .. } => {
                self.prepare_window_create_original_locked(session, active, correlation, request)
            }
            WindowRequest::LoadUrl { window, uri } => {
                self.prepare_window_original_load_locked(session, active, correlation, window, &uri)
            }
            WindowRequest::SetMaximized { window, maximized } => {
                let outcome = session.registry.set_maximized(caller, window, maximized);
                self.prepare_window_setter_result(outcome)
            }
            WindowRequest::SetFullScreen {
                window,
                full_screen,
            } => {
                let outcome = session
                    .registry
                    .set_full_screen(caller, window, full_screen);
                self.prepare_window_setter_result(outcome)
            }
            WindowRequest::RequestClose { window } => {
                Ok(match session.registry.request_close(caller, window) {
                    Ok(outcome) => WindowRequestOutcome::Close(outcome),
                    Err(error) => WindowRequestOutcome::Refused(WindowRefusal::Registry(error)),
                })
            }
            WindowRequest::CloseReply {
                window,
                close_seq,
                verdict,
            } => self.prepare_window_close_reply_locked(
                session,
                active,
                window,
                close_seq,
                verdict,
                #[cfg(windows)]
                close_control,
            ),
            WindowRequest::DestroyOriginalDocument {
                window,
                document_generation,
            } => Ok(self.prepare_window_original_destroy_locked(
                session,
                active,
                correlation,
                window,
                document_generation,
            )),
            WindowRequest::Destroy { window } => self.prepare_window_destroy_locked(
                session,
                active,
                correlation,
                window,
                #[cfg(windows)]
                close_control,
            ),
        }
    }

    fn prepare_window_create_result(
        &self,
        outcome: Result<CreateOutcome, WindowError>,
        #[cfg(any(target_os = "macos", windows))] resources: WindowBuildResources,
        #[cfg(not(any(target_os = "macos", windows)))] _resources: WindowBuildResources,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        match outcome {
            Ok(CreateOutcome::Build {
                build,
                width,
                height,
                x,
                y,
                ..
            }) => {
                self.window_commands
                    .send(AppWindowCommand::CreateWindow {
                        build,
                        width,
                        height,
                        x,
                        y,
                        #[cfg(any(target_os = "macos", windows))]
                        preload_image: resources.preload_image,
                        #[cfg(any(target_os = "macos", windows))]
                        original_source: resources.original_source,
                        #[cfg(windows)]
                        original_role: resources.original_role,
                    })
                    .map_err(|_| {
                        app_detail("window native build", "UI command owner is unavailable")
                    })?;
                Ok(WindowRequestOutcome::Deferred)
            }
            Ok(CreateOutcome::Adopted(window)) => {
                Ok(WindowRequestOutcome::Reply(WindowResponse::Adopted {
                    window,
                }))
            }
            Err(error) => Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                error,
            ))),
        }
    }

    fn prepare_window_setter_result(
        &self,
        outcome: Result<NativeSetter, WindowError>,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        let setter = match outcome {
            Ok(setter) => setter,
            Err(error) => {
                return Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                    error,
                )));
            }
        };
        let command = match setter {
            NativeSetter::Maximized { view, maximized } => {
                AppWindowCommand::SetMaximized { view, maximized }
            }
            NativeSetter::FullScreen { view, full_screen } => {
                AppWindowCommand::SetFullScreen { view, full_screen }
            }
        };
        self.window_commands
            .send(command)
            .map_err(|_| app_detail("window setter", "UI command owner is unavailable"))?;
        Ok(WindowRequestOutcome::Reply(WindowResponse::Accepted))
    }
}
