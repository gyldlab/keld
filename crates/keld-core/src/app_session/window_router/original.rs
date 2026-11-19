//! Original Create preparation for the authenticated, ordered Window publisher.

use std::sync::Arc;
use std::time::{Duration, Instant};

use keld_ipc::window::WindowRef;

use super::super::window_registry::OriginalWindowResources;
use super::{
    AppWindowCommand, CorrelationId, CreateOutcome, HostAppError, PrimaryCallerContext,
    PrimaryRouterHandle, WindowError, WindowRequest, WindowSession,
};

// The approved GH-528 original Create cut starts at the publication owner's
// receipt of the call, before waiting for its Window source turn.
const ORIGINAL_CREATE_START_LIMIT: Duration = Duration::from_secs(35);

#[derive(Debug)]
pub(in crate::app_session) enum OriginalWindowCreateRefusal {
    Unavailable,
    WrongRequest,
    Preload(HostAppError),
    Deadline,
    Registry(WindowError),
}

#[derive(Debug)]
pub(in crate::app_session) enum PreparedOriginalWindowCreate {
    Build(AppWindowCommand),
    Adopted(WindowRef),
}

impl PrimaryRouterHandle {
    /// Caller holds transition -> authenticated current generation -> Window
    /// Registry. The ordered publication owner retains or emits this outcome.
    pub(in crate::app_session) fn prepare_original_window_create_locked(
        &self,
        session: &mut WindowSession,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        request: WindowRequest,
        started: Instant,
    ) -> Result<PreparedOriginalWindowCreate, OriginalWindowCreateRefusal> {
        use OriginalWindowCreateRefusal::{Deadline, Preload, Registry, Unavailable, WrongRequest};
        let WindowRequest::CreateWithPreload {
            width,
            height,
            x,
            y,
            preload_absolute,
            additional_arguments,
        } = request
        else {
            return Err(WrongRequest);
        };
        let selection = session.original_selection.as_ref().ok_or(Unavailable)?;
        let role = self.renderer_role_scope(caller).ok_or(Unavailable)?;
        let cutoff = started + ORIGINAL_CREATE_START_LIMIT;
        if Instant::now() >= cutoff {
            return Err(Deadline);
        }
        let resources = selection
            .prepare_preload(&preload_absolute, additional_arguments)
            .map_err(Preload)?;
        let application_name = selection.application_name_for_user_agent().cloned();
        if Instant::now() >= cutoff {
            return Err(Deadline);
        }
        let outcome = session
            .create_original_for_caller(
                caller,
                correlation,
                (width, height),
                (x, y),
                OriginalWindowResources {
                    source: Arc::clone(&resources.source),
                    preload: Arc::clone(&resources.preload),
                },
            )
            .map_err(Registry)?;
        Ok(match outcome {
            CreateOutcome::Build {
                build,
                window,
                width,
                height,
                x,
                y,
            } => PreparedOriginalWindowCreate::Build(AppWindowCommand::CreateWindow {
                build,
                width,
                height,
                x,
                y,
                original_source: Some(resources.source),
                preload_image: Some(resources.preload),
                original_role: Some(role.for_window(window)),
                original_application_name_for_user_agent: application_name,
                original_create_start_by: Some(cutoff),
            }),
            CreateOutcome::Adopted(window) => PreparedOriginalWindowCreate::Adopted(window),
        })
    }
}
