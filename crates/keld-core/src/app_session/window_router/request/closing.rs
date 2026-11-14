//! Logical close admission remains in the one Registry. Its exact physical
//! cleanup is retained until preceding terminal publication attempts finish.
use super::{
    ActivePrimaryGeneration, CloseOutcome, ClosingRequest, CorrelationId, HostAppError,
    PrimaryRouterHandle, WindowRefusal, WindowRequestOutcome, WindowSession, app_detail,
    append_app_cleanup, owner, report_abandoned,
};
#[cfg(any(target_os = "macos", windows))]
use super::{AppWindowCommand, WindowError};

#[cfg(windows)]
pub(in crate::app_session::window_router) enum WindowCloseControl {
    Reserved(Option<keld_wv::renderer_bridge::NativeControlReservation>),
    #[cfg(test)]
    LegacyPolicy,
}

impl PrimaryRouterHandle {
    pub(super) fn prepare_window_destroy_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        window: keld_ipc::window::WindowRef,
        #[cfg(windows)] close_control: &mut WindowCloseControl,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        #[cfg(any(target_os = "macos", windows))]
        let mut availability = Ok(());
        let outcome = session.registry.destroy_with_retirement(
            owner(active),
            window,
            correlation,
            report_abandoned,
            |registry| {
                #[cfg(target_os = "macos")]
                {
                    availability =
                        self.window_registered_route_retirement_locked(registry, active, window);
                }
                #[cfg(windows)]
                {
                    availability = match close_control {
                        WindowCloseControl::Reserved(control) => self
                            .window_registered_route_retirement_reserved_locked(
                                registry, active, window, control,
                            ),
                        #[cfg(test)]
                        WindowCloseControl::LegacyPolicy => {
                            self.window_registered_route_retirement_locked(registry, active, window)
                        }
                    };
                }
                #[cfg(not(any(target_os = "macos", windows)))]
                let _ = registry;
            },
        );
        #[cfg(not(any(target_os = "macos", windows)))]
        let availability = Ok(());
        match outcome {
            Ok(CloseOutcome::Teardown(teardown)) => {
                #[cfg(any(target_os = "macos", windows))]
                let scoped = session.registry.cancel_original_document_destroy(window);
                #[cfg(any(target_os = "macos", windows))]
                let interrupted = session.registry.take_interrupted_original_load(window);
                #[cfg(not(any(target_os = "macos", windows)))]
                let interrupted = None;
                Ok(WindowRequestOutcome::Closing(ClosingRequest {
                    availability,
                    #[cfg(any(target_os = "macos", windows))]
                    scoped,
                    interrupted,
                    acknowledge: false,
                    teardown,
                }))
            }
            Ok(CloseOutcome::AlreadyClosing) => {
                availability.map(|()| WindowRequestOutcome::Deferred)
            }
            Ok(_) => Err(append_app_cleanup(
                app_detail(
                    "window Destroy",
                    "registry returned an invalid destruction effect",
                ),
                [availability],
            )),
            Err(error) => {
                availability?;
                Ok(WindowRequestOutcome::Refused(WindowRefusal::Registry(
                    error,
                )))
            }
        }
    }

    pub(super) fn prepare_window_close_reply_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        window: keld_ipc::window::WindowRef,
        close_seq: u64,
        verdict: keld_ipc::window::CloseVerdict,
        #[cfg(windows)] close_control: &mut WindowCloseControl,
    ) -> Result<WindowRequestOutcome, HostAppError> {
        #[cfg(any(target_os = "macos", windows))]
        let mut availability = Ok(());
        let outcome = session.registry.close_reply_with_retirement(
            owner(active),
            window,
            close_seq,
            verdict,
            |registry| {
                #[cfg(target_os = "macos")]
                {
                    availability =
                        self.window_registered_route_retirement_locked(registry, active, window);
                }
                #[cfg(windows)]
                {
                    availability = match close_control {
                        WindowCloseControl::Reserved(control) => self
                            .window_registered_route_retirement_reserved_locked(
                                registry, active, window, control,
                            ),
                        #[cfg(test)]
                        WindowCloseControl::LegacyPolicy => {
                            self.window_registered_route_retirement_locked(registry, active, window)
                        }
                    };
                }
                #[cfg(not(any(target_os = "macos", windows)))]
                let _ = registry;
            },
        );
        #[cfg(not(any(target_os = "macos", windows)))]
        let availability = Ok(());
        if let Ok(CloseOutcome::Teardown(teardown)) = outcome {
            #[cfg(any(target_os = "macos", windows))]
            let scoped = session.registry.cancel_original_document_destroy(window);
            #[cfg(any(target_os = "macos", windows))]
            let interrupted = session.registry.take_interrupted_original_load(window);
            #[cfg(not(any(target_os = "macos", windows)))]
            let interrupted = None;
            return Ok(WindowRequestOutcome::Closing(ClosingRequest {
                availability,
                #[cfg(any(target_os = "macos", windows))]
                scoped,
                interrupted,
                acknowledge: true,
                teardown,
            }));
        }
        availability?;
        Ok(match outcome {
            Ok(close) => WindowRequestOutcome::Close(close),
            Err(error) => WindowRequestOutcome::Refused(WindowRefusal::Registry(error)),
        })
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub(super) fn prepare_window_original_destroy_locked(
        &self,
        _session: &mut WindowSession,
        _active: &ActivePrimaryGeneration,
        _correlation: CorrelationId,
        _window: keld_ipc::window::WindowRef,
        _document_generation: u32,
    ) -> WindowRequestOutcome {
        WindowRequestOutcome::Refused(WindowRefusal::fixed(
            "KELD-CORE-042",
            "KELD-CORE-042: original-document teardown requires its qualified native adapter.",
        ))
    }

    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn prepare_window_original_destroy_locked(
        &self,
        session: &mut WindowSession,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        window: keld_ipc::window::WindowRef,
        document_generation: u32,
    ) -> WindowRequestOutcome {
        let caller = owner(active);
        let view = match session.registry.admitted_dialog_view(caller, window) {
            Ok(view) => view,
            Err(error) => {
                return WindowRequestOutcome::Refused(WindowRefusal::Registry(error));
            }
        };
        let expected = if document_generation == 0 {
            None
        } else {
            session
                .registry
                .renderer_route(caller, view, Some(document_generation))
                .ok()
                .map(|route| route.document)
        };
        let Some(expected) = expected else {
            return WindowRequestOutcome::Refused(WindowRefusal::Registry(
                WindowError::OriginalDocumentCloseRefused,
            ));
        };
        let admission = std::sync::Arc::new(
            keld_wv::renderer_contract::OriginalDocumentDestroyAdmission::new(),
        );
        if let Err(error) = session.registry.reserve_original_document_destroy(
            caller,
            correlation,
            window,
            std::sync::Arc::clone(&admission),
        ) {
            return WindowRequestOutcome::Refused(WindowRefusal::Registry(error));
        }
        if self
            .window_commands
            .send(AppWindowCommand::DestroyOriginalDocument {
                view,
                expected,
                admission: std::sync::Arc::clone(&admission),
            })
            .is_err()
        {
            let _ = admission.cancel_before_admission();
            let outcome = session
                .registry
                .original_document_destroy_decision(view, &admission);
            return WindowRequestOutcome::OriginalDestroy(outcome);
        }
        // A queued conditional request changes no phase/document/Ready state.
        // The native admission and actual Released remain the only success facts.
        WindowRequestOutcome::Deferred
    }
}
