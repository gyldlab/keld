//! Shared original-source request admission. The native queue remains the
//! effect owner; only actual native milestones produce success replies.
#[cfg(target_os = "macos")]
use super::PendingWindowCall;
use super::{
    ActivePrimaryGeneration, CorrelationId, HostAppError, PrimaryRouterHandle, WindowRefusal,
    WindowRequest, WindowRequestOutcome, WindowSession,
};
#[cfg(any(target_os = "macos", windows))]
use super::{
    AppWindowCommand, OriginalLoadAdmission, WindowBuildResources, WindowError, app_detail, owner,
};
#[cfg(windows)]
use super::{CreateOutcome, PrimaryCallerContext};

impl PrimaryRouterHandle {
    #[cfg(not(any(target_os = "macos", windows)))]
    pub(super) fn prepare_window_create_original_locked(
        &self,
        _session: &mut WindowSession,
        _active: &ActivePrimaryGeneration,
        _correlation: CorrelationId,
        _request: WindowRequest,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        Ok(WindowRequestOutcome::Refused(WindowRefusal::fixed(
            "KELD-CORE-042",
            "KELD-CORE-042: original preload creation requires its qualified macOS adapter.",
        )))
    }

    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn prepare_window_create_original_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        request: WindowRequest,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        let WindowRequest::CreateWithPreload {
            width,
            height,
            x,
            y,
            preload_absolute,
            additional_arguments,
        } = request
        else {
            return Err(app_detail(
                "original Create dispatch",
                "shared decoder routed a non-preload request to preload admission",
            ));
        };
        let Some(selection) = session
            .original_selection
            .as_ref()
            .map(std::sync::Arc::clone)
        else {
            return Ok(WindowRequestOutcome::Refused(WindowRefusal::fixed(
                "KELD-CORE-051",
                "KELD-CORE-051: verified source and original preload selection are unavailable; prepare the graph and resolved window policy before CreateWithPreload.",
            )));
        };
        let preload_image = match selection.prepare_preload(&preload_absolute, additional_arguments)
        {
            Ok(image) => image,
            Err(error) => {
                return Ok(WindowRequestOutcome::Refused(
                    WindowRefusal::OriginalPreload(error),
                ));
            }
        };
        #[cfg(not(windows))]
        let outcome = session.registry.create_with_original_preload(
            PendingWindowCall {
                owner: owner(active),
                correlation,
            },
            (width, height),
            (x, y),
            std::sync::Arc::clone(&selection),
            std::sync::Arc::clone(&preload_image),
        );
        #[cfg(windows)]
        let outcome = session.registry.create_with_original_preload_for_caller(
            &PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: std::sync::Arc::clone(&active.reader_stop),
            },
            correlation,
            (width, height),
            (x, y),
            std::sync::Arc::clone(&selection),
            std::sync::Arc::clone(&preload_image),
        );
        let resources = WindowBuildResources {
            preload_image: Some(preload_image),
            original_source: Some(std::sync::Arc::clone(selection.source())),
            #[cfg(windows)]
            original_role: outcome.as_ref().ok().and_then(|outcome| match outcome {
                CreateOutcome::Build { window, .. } => Some(self.renderer_role_witness_for_window(
                    &PrimaryCallerContext {
                        attempt: active.attempt,
                        identity: active.identity,
                        reader_stop: std::sync::Arc::clone(&active.reader_stop),
                    },
                    *window,
                )),
                CreateOutcome::Adopted(_) => None,
            }),
        };
        self.prepare_window_create_result(outcome, resources)
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub(super) fn prepare_window_original_load_locked(
        &self,
        _session: &mut WindowSession,
        _active: &ActivePrimaryGeneration,
        _correlation: CorrelationId,
        _window: keld_ipc::window::WindowRef,
        _uri: &str,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        Ok(WindowRequestOutcome::Refused(WindowRefusal::fixed(
            "KELD-CORE-042",
            "KELD-CORE-042: original editor loading requires its qualified macOS native adapter.",
        )))
    }

    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn prepare_window_original_load_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        window: keld_ipc::window::WindowRef,
        uri: &str,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        // Validate the real pair/owner before source information or native work.
        if let Err(error) = session.registry.admitted_dialog_view(owner(active), window) {
            return Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                error,
            )));
        }
        let selection = match session
            .registry
            .original_window_source(owner(active), window)
        {
            Ok(selection) => selection,
            Err(error) => {
                return Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                    error,
                )));
            }
        };
        let query = match selection.qualify_uri(uri) {
            Ok(query) => query,
            Err(error) => {
                return Ok(WindowRequestOutcome::Refused(WindowRefusal::OriginalUri(
                    error,
                )));
            }
        };
        let (operation, view) =
            match session
                .registry
                .admit_original_load(owner(active), correlation, window)
            {
                Ok(OriginalLoadAdmission::Dispatch {
                    operation, view, ..
                }) => (operation, view),
                Ok(OriginalLoadAdmission::Busy) => {
                    return Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                        WindowError::OriginalLoadBusy,
                    )));
                }
                Err(error) => {
                    return Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                        error,
                    )));
                }
            };
        let Some(operation_id) = std::num::NonZeroU64::new(operation) else {
            session.registry.abandon_original_dispatch(operation);
            return Err(app_detail(
                "original load correlation",
                "checked operation was zero",
            ));
        };
        let context = keld_wv::NativeNavigationContext::new(operation_id, view);
        if self
            .window_commands
            .send(AppWindowCommand::LoadAdmittedOriginal {
                context,
                query,
                #[cfg(windows)]
                role_witness: self.renderer_role_witness_for_window(
                    &PrimaryCallerContext {
                        attempt: active.attempt,
                        identity: active.identity,
                        reader_stop: std::sync::Arc::clone(&active.reader_stop),
                    },
                    window,
                ),
            })
            .is_err()
        {
            session.registry.abandon_original_dispatch(operation);
            return Ok(WindowRequestOutcome::Refused(WindowRefusal::fixed(
                "KELD-CORE-051",
                "KELD-CORE-051: native UI admission is unavailable; restart the Host before another original load.",
            )));
        }
        Ok(WindowRequestOutcome::Deferred)
    }
}
