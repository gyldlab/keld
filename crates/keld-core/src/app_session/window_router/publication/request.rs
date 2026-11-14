//! Typed canonical request outcomes retain their established publication order.
use super::super::super::window_registry::{ClosingCleanup, Teardown};
use super::super::request::ClosingRequest;
use super::{
    ActivePrimaryGeneration, Arc, CloseOutcome, CorrelationId, HostAppError, PrimaryCallerContext,
    PrimaryRouterHandle, PublicationError, PublicationResult, WindowEmitter, WindowRef,
    WindowRegistry, WindowRequestOutcome, WindowResponse, app_detail, owner,
};

impl WindowEmitter<'_, '_> {
    pub(super) fn request(
        &self,
        correlation: CorrelationId,
        outcome: WindowRequestOutcome,
    ) -> PublicationResult<()> {
        match outcome {
            WindowRequestOutcome::Deferred => Ok(()),
            WindowRequestOutcome::Reply(response) => self.reply(correlation, response, None),
            WindowRequestOutcome::Refused(refusal) => {
                self.refusal(correlation, refusal, false, None)
            }
            WindowRequestOutcome::Close(close) => self.close_reply(correlation, close),
            WindowRequestOutcome::OriginalDestroy(resolution) => {
                self.original_destroy_resolution(&resolution, None)
            }
            WindowRequestOutcome::Closing(action) => self.closing(correlation, action),
        }
    }

    fn close_reply(
        &self,
        correlation: CorrelationId,
        close: CloseOutcome,
    ) -> PublicationResult<()> {
        let response = match close {
            CloseOutcome::Requested {
                close_seq,
                fact: Some(fact),
            } => {
                self.fact(&fact)?;
                WindowResponse::CloseRequested { close_seq }
            }
            CloseOutcome::Merged { close_seq } => WindowResponse::Merged { close_seq },
            CloseOutcome::AlreadyClosing => WindowResponse::AlreadyClosing,
            CloseOutcome::Vetoed => WindowResponse::ReplyApplied,
            CloseOutcome::Stale => {
                eprintln!(
                    "KELD-CORE-038: stale close reply ignored; answer the current close_seq once"
                );
                WindowResponse::ReplyStale
            }
            CloseOutcome::Teardown(action) => {
                let written = self.reply(
                    correlation,
                    WindowResponse::ReplyApplied,
                    Some(action.window),
                );
                let cleanup = self.router.cleanup_window_action(action, "Window teardown");
                return with_cleanup(written, cleanup);
            }
            CloseOutcome::Requested { fact: None, .. }
            | CloseOutcome::Unmapped
            | CloseOutcome::Exhausted => {
                return Err(app_detail(
                    "Window close reply",
                    "admitted call returned a native-only close outcome",
                )
                .into());
            }
        };
        self.reply(correlation, response, None)
    }

    fn closing(&self, correlation: CorrelationId, action: ClosingRequest) -> PublicationResult<()> {
        let result = (|| {
            action.availability?;
            self.original_destroy_resolution(&action.scoped, Some(action.teardown.window))?;
            self.original_interruption(action.interrupted, Some(action.teardown.window))?;
            if action.acknowledge {
                self.reply(
                    correlation,
                    WindowResponse::ReplyApplied,
                    Some(action.teardown.window),
                )?;
            }
            Ok(())
        })();
        // A partial/unknown frame cannot be followed by another frame. Required
        // native cleanup nevertheless survives every publication failure.
        let cleanup = self
            .router
            .cleanup_window_action(action.teardown, "Window teardown");
        with_cleanup(result, cleanup)
    }
}

pub(super) fn with_cleanup(
    result: PublicationResult<()>,
    cleanup: Result<(), HostAppError>,
) -> PublicationResult<()> {
    match result {
        Ok(()) => cleanup.map_err(PublicationError::from),
        Err(error) => Err(error.with_cleanup(cleanup)),
    }
}

impl PrimaryRouterHandle {
    pub(in crate::app_session::window_router) fn cleanup_window_action(
        &self,
        action: Teardown,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail(phase, "current lock poisoned"))?;
        let windows = self
            .windows
            .as_ref()
            .ok_or_else(|| app_detail(phase, "Registry disappeared"))?;
        let session = windows
            .lock()
            .map_err(|_| app_detail(phase, "Registry lock poisoned"))?;
        match session
            .registry
            .closing_cleanup(action)
            .map_err(|error| app_detail(phase, error.to_string()))?
        {
            ClosingCleanup::Required => self.send_window_teardown(action, phase),
            ClosingCleanup::Released => Ok(()),
        }
    }
}

impl PrimaryRouterHandle {
    pub(in crate::app_session::window_router) fn window_registered_route_retirement_reserved_locked(
        &self,
        registry: &WindowRegistry,
        active: &ActivePrimaryGeneration,
        window: WindowRef,
        control: &mut Option<keld_wv::renderer_bridge::NativeControlReservation>,
    ) -> Result<(), HostAppError> {
        let caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        if !registry.retains_caller_window(&caller, window) {
            return Ok(());
        }
        registry.for_each_registered_renderer_route(owner(active), |route| {
            if route.window == window {
                self.compat_route_retired_reserved_locked(&caller, &route, control)?;
            }
            Ok(())
        })
    }
}
