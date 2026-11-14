//! Shared native fact admission. Registry transitions occur once; the caller
//! retains each typed outcome through its ordered synchronous publication.
use super::{
    AppWindowEvent, BuildFailureOutcome, BuildOutcome, BuildStage, CloseOutcome, FactOutcome,
    HostAppError, PrimaryRouterHandle, Rect, ReleaseOutcome, WindowSession, app_detail,
    native_state,
};

#[cfg(any(not(windows), test))]
mod legacy;

#[cfg(any(target_os = "macos", windows))]
use super::original_refusal_detail;

#[cfg(any(target_os = "macos", windows))]
use super::super::window_registry::CommittedOriginalLoad;

pub(super) enum NativeOutcome {
    None,
    Fact(FactOutcome),
    Build(BuildOutcome),
    BuildFailed {
        outcome: BuildFailureOutcome,
        stage: BuildStage,
    },
    Released(ReleaseOutcome),
    Close(CloseOutcome),
    #[cfg(any(target_os = "macos", windows))]
    OriginalDestroy {
        resolution: super::super::window_registry::OriginalDocumentDestroyResolution,
        admission: std::sync::Arc<keld_wv::renderer_contract::OriginalDocumentDestroyAdmission>,
    },
    #[cfg(any(target_os = "macos", windows))]
    Committed(CommittedOriginalLoad),
    #[cfg(any(target_os = "macos", windows))]
    Finished(super::super::window_registry::FinishedOriginalLoad),
    #[cfg(any(target_os = "macos", windows))]
    OriginalFailure {
        call: Option<super::super::window_registry::PendingWindowCall>,
        code: &'static str,
        detail: &'static str,
    },
}

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the closed native fact match and its exact Registry transition in one caller-held admission operation."
    )]
    pub(super) fn prepare_window_native_locked(
        session: &mut WindowSession,
        native_event: AppWindowEvent,
    ) -> Result<NativeOutcome, HostAppError> {
        let outcome = match native_event {
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::DesktopObserved { .. } => {
                return Err(app_detail(
                    "window native event",
                    "desktop observation must be handled by its selected source owner",
                ));
            }
            #[cfg(windows)]
            AppWindowEvent::WindowsDesktopObserved { .. } => {
                return Err(app_detail(
                    "window native event",
                    "scoped desktop observation must use its exact-reader source owner",
                ));
            }
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::OriginalDocumentDestroyDecision { view, admission } => {
                let resolution = session
                    .registry
                    .original_document_destroy_decision(view, &admission);
                NativeOutcome::OriginalDestroy {
                    resolution,
                    admission,
                }
            }
            #[cfg(target_os = "macos")]
            AppWindowEvent::MacOriginalNavigation { context, fact } => {
                return Ok(Self::prepare_window_original_native_locked(
                    session, context, fact,
                ));
            }
            #[cfg(windows)]
            AppWindowEvent::OriginalNavigation { context, fact } => {
                return Ok(Self::prepare_window_original_native_locked(
                    session, context, fact,
                ));
            }
            AppWindowEvent::WindowBuilt {
                build,
                view,
                snapshot,
            } => NativeOutcome::Build(session.registry.native_built(
                build,
                view,
                native_state(snapshot),
            )),
            AppWindowEvent::WindowBuildFailed { build, stage } => NativeOutcome::BuildFailed {
                outcome: session.registry.native_failed(build),
                stage,
            },
            AppWindowEvent::WindowReleased { view } => {
                NativeOutcome::Released(session.registry.native_released(view))
            }
            AppWindowEvent::WindowCloseRequested { view } => {
                NativeOutcome::Close(session.registry.native_close(view))
            }
            AppWindowEvent::WindowBounds {
                view,
                x,
                y,
                width,
                height,
            } => NativeOutcome::Fact(session.registry.native_bounds(
                view,
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
            )),
            AppWindowEvent::WindowFocus { view, focused } => {
                NativeOutcome::Fact(session.registry.native_focus(view, focused))
            }
            AppWindowEvent::WindowMaximized { view, maximized } => {
                NativeOutcome::Fact(session.registry.native_maximized(view, maximized))
            }
            AppWindowEvent::WindowFullScreen { view, full_screen } => {
                NativeOutcome::Fact(session.registry.native_full_screen(view, full_screen))
            }
            AppWindowEvent::NavigationReady | AppWindowEvent::LastWindowClosed => {
                return Err(app_detail(
                    "window native event",
                    "legacy lifecycle fact entered managed registry dispatch",
                ));
            }
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::HostInitialized { .. } => {
                return Err(app_detail(
                    "window native event",
                    "the first host Init milestone belongs only to the lifecycle coordinator",
                ));
            }
            #[cfg(windows)]
            AppWindowEvent::HostWriterProgress => {
                return Err(app_detail(
                    "window native event",
                    "writer progress must drain retained records before Registry dispatch",
                ));
            }
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::DialogFinished { .. } => {
                return Err(app_detail(
                    "window native event",
                    "dialog completion must be dispatched by the event coordinator",
                ));
            }
        };
        Ok(outcome)
    }

    #[cfg(any(target_os = "macos", windows))]
    fn prepare_window_original_native_locked(
        session: &mut WindowSession,
        context: keld_wv::NativeNavigationContext,
        fact: keld_wv::OriginalNavigationFact,
    ) -> NativeOutcome {
        use keld_wv::OriginalNavigationFact;
        let (call, code, detail) = match fact {
            OriginalNavigationFact::Started { attempt } => {
                session.registry.original_load_started(context, attempt);
                return NativeOutcome::None;
            }
            OriginalNavigationFact::Committed {
                stamp,
                native_url,
                document,
            } => {
                let Some(committed) = session
                    .registry
                    .original_load_committed(context, stamp, document, native_url)
                else {
                    return NativeOutcome::None;
                };
                return NativeOutcome::Committed(committed);
            }
            OriginalNavigationFact::Finished { stamp } => {
                return session
                    .registry
                    .original_load_finished(context, stamp)
                    .map_or(NativeOutcome::None, NativeOutcome::Finished);
            }
            OriginalNavigationFact::Refused { error } => (
                session.registry.original_load_failed(context, None),
                "KELD-CORE-051",
                original_refusal_detail(error),
            ),
            OriginalNavigationFact::Failed { attempt, revoked } => {
                if let Some(previous) = revoked {
                    session
                        .registry
                        .renderer_document_revoked(context.view(), previous);
                }
                (
                    session
                        .registry
                        .original_load_failed(context, Some(attempt)),
                    "KELD-CORE-052",
                    "the actual admitted native navigation failed",
                )
            }
            OriginalNavigationFact::Revoked { previous } => {
                let Some(previous) = previous else {
                    return NativeOutcome::None;
                };
                session
                    .registry
                    .renderer_document_revoked(context.view(), previous);
                (
                    session
                        .registry
                        .original_load_failed(context, Some(previous.attempt())),
                    "KELD-CORE-052",
                    "the admitted native document was revoked",
                )
            }
        };
        NativeOutcome::OriginalFailure { call, code, detail }
    }
}
