//! macOS and historical no-worker fixtures keep their existing guarded writer.
//! Typed policy extraction preserves each native-action/publication ordering.
use super::super::{
    ActivePrimaryGeneration, CorrelationId, HostAppError, PrimaryRouterHandle, WindowResponse,
    WindowSession, call_error, collapse_app_results, window_reply,
};
use super::WindowRequestOutcome;

impl PrimaryRouterHandle {
    pub(in crate::app_session::window_router) fn publish_window_request_locked(
        &self,
        session: &mut WindowSession,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        outcome: WindowRequestOutcome,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            WindowRequestOutcome::Deferred => Ok(()),
            WindowRequestOutcome::Reply(response) => {
                window_reply(active, correlation, response, failed)
            }
            WindowRequestOutcome::Refused(refusal) => {
                let (code, message) = refusal.into_parts();
                call_error(active, correlation, code, message, failed)
            }
            WindowRequestOutcome::Close(close) => self.window_close_call_locked(
                active,
                correlation,
                close,
                &mut session.navigation_payload,
                failed,
            ),
            #[cfg(any(target_os = "macos", windows))]
            WindowRequestOutcome::OriginalDestroy(outcome) => {
                Self::window_original_destroy_resolution(active, &outcome, failed)
            }
            WindowRequestOutcome::Closing(outcome) => {
                #[cfg(any(target_os = "macos", windows))]
                let scoped =
                    Self::window_original_destroy_resolution(active, &outcome.scoped, failed);
                #[cfg(not(any(target_os = "macos", windows)))]
                let scoped = Ok(());
                let interrupted =
                    Self::window_original_interruption(active, outcome.interrupted, failed);
                let acknowledgement = if outcome.acknowledge {
                    window_reply(active, correlation, WindowResponse::ReplyApplied, failed)
                } else {
                    Ok(())
                };
                // All previous attempts are evaluated before the same physical
                // teardown, even when a terminal write or availability failed.
                let teardown = self.send_window_teardown(outcome.teardown, "window teardown");
                collapse_app_results([
                    outcome.availability,
                    scoped,
                    interrupted,
                    acknowledgement,
                    teardown,
                ])
            }
        }
    }
}
