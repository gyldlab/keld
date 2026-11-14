//! Existing guarded publication order for macOS and historical fixtures.
use super::super::{
    ActivePrimaryGeneration, FactOutcome, HostAppError, PrimaryRouterHandle, WindowSession,
    owned_event, window_build_failed_locked, window_released_locked,
};
#[cfg(any(target_os = "macos", windows))]
use super::super::{
    PrimaryCallerContext, active_owner, app_detail, collapse_app_results,
    navigation_committed_event, original_close_refusal, original_native_failure, owned_reply,
    report_abandoned,
};
use super::NativeOutcome;

impl PrimaryRouterHandle {
    #[cfg_attr(
        windows,
        expect(
            clippy::too_many_lines,
            reason = "Preserve the complete existing legacy native-outcome dispatch and its owned success/failure carriers."
        )
    )]
    pub(in crate::app_session::window_router) fn publish_window_native_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        outcome: NativeOutcome,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let subscribed = session.subscribed;
        match outcome {
            NativeOutcome::None => Ok(()),
            NativeOutcome::Fact(fact) => match fact {
                FactOutcome::Event(fact) => owned_event(
                    current,
                    subscribed,
                    &fact,
                    &mut session.navigation_payload,
                    failed,
                ),
                FactOutcome::Unmapped => {
                    eprintln!(
                        "KELD-CORE-038: native state references an unmapped view; inspect UI event ordering"
                    );
                    Ok(())
                }
                FactOutcome::Coalesced | FactOutcome::Suppressed | FactOutcome::Dropped => Ok(()),
            },
            NativeOutcome::Build(outcome) => self.window_build_outcome_locked(
                current,
                subscribed,
                outcome,
                &mut session.navigation_payload,
                failed,
            ),
            NativeOutcome::BuildFailed { outcome, stage } => {
                window_build_failed_locked(current, subscribed, outcome, stage, failed)
            }
            NativeOutcome::Released(outcome) => window_released_locked(
                current,
                subscribed,
                outcome,
                &mut session.navigation_payload,
                failed,
            ),
            NativeOutcome::Close(outcome) => Self::window_native_close_locked(
                current,
                subscribed,
                outcome,
                &mut session.navigation_payload,
                failed,
            ),
            #[cfg(any(target_os = "macos", windows))]
            NativeOutcome::OriginalDestroy {
                resolution,
                admission,
            } => {
                if let Some(close) = resolution.abandoned {
                    report_abandoned(close);
                }
                let refused = if let Some(call) = resolution.refused
                    && subscribed == Some(call.owner)
                    && let Some(active) = active_owner(current, call.owner)
                {
                    original_close_refusal(active, call.correlation, failed)
                } else {
                    Ok(())
                };
                let interrupted = if let Some(call) = resolution.interrupted
                    && subscribed == Some(call.owner)
                    && let Some(active) = active_owner(current, call.owner)
                {
                    Self::window_original_interruption(active, Some(call), failed)
                } else {
                    Ok(())
                };
                let result = collapse_app_results([refused, interrupted]);
                drop(admission); // exact native ticket survives both publication attempts
                result
            }
            #[cfg(any(target_os = "macos", windows))]
            NativeOutcome::Committed(committed) => {
                if subscribed == Some(committed.owner)
                    && let Some(active) = active_owner(current, committed.owner)
                {
                    let native_url = session
                        .registry
                        .committed_native_url(
                            committed.owner,
                            committed.window,
                            committed.document_generation,
                        )
                        .ok_or_else(|| {
                            app_detail(
                                "original native commit",
                                "the same guarded Registry URL disappeared before publication",
                            )
                        })?;
                    navigation_committed_event(
                        active,
                        committed.window,
                        committed.document_generation,
                        native_url.as_str(),
                        &mut session.navigation_payload,
                        failed,
                    )?;
                    let caller = PrimaryCallerContext {
                        identity: active.identity,
                        attempt: active.attempt,
                        reader_stop: std::sync::Arc::clone(&active.reader_stop),
                    };
                    session.registry.for_each_registered_renderer_route(
                        committed.owner,
                        |route| {
                            if route.window == committed.window {
                                self.compat_registered_route_ready_locked(&caller, active, route)?;
                            }
                            Ok(())
                        },
                    )?;
                }
                Ok(())
            }
            #[cfg(any(target_os = "macos", windows))]
            NativeOutcome::Finished(completed) => {
                owned_event(
                    current,
                    subscribed,
                    &completed.fact,
                    &mut session.navigation_payload,
                    failed,
                )?;
                owned_reply(current, subscribed, completed.reply, failed)
            }
            #[cfg(any(target_os = "macos", windows))]
            NativeOutcome::OriginalFailure { call, code, detail } => {
                original_native_failure(current, subscribed, (call, code, detail), failed)
            }
        }
    }
}
