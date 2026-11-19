//! Apply each native receipt once in the existing Registry before publication.

use super::{
    AppWindowEvent, BuildFailureOutcome, BuildOutcome, BuildStage, CloseOutcome, FactOutcome,
    HostAppError, PrimaryRouterHandle, Rect, ReleaseOutcome, WindowSession, app_detail,
    native_state,
};

pub(super) enum NativeOutcome {
    Build(BuildOutcome),
    BuildFailed {
        outcome: BuildFailureOutcome,
        stage: BuildStage,
    },
    Close(CloseOutcome),
    Released(ReleaseOutcome),
    Fact(FactOutcome),
}

impl PrimaryRouterHandle {
    pub(super) fn prepare_window_native_locked(
        session: &mut WindowSession,
        native_event: AppWindowEvent,
    ) -> Result<NativeOutcome, HostAppError> {
        let registry = &mut session.registry;
        Ok(match native_event {
            AppWindowEvent::WindowBuilt {
                build,
                view,
                snapshot,
            } => NativeOutcome::Build(registry.native_built(build, view, native_state(snapshot))),
            AppWindowEvent::WindowBuildFailed { build, stage } => NativeOutcome::BuildFailed {
                outcome: registry.native_failed(build),
                stage,
            },
            AppWindowEvent::WindowCloseRequested { view } => {
                NativeOutcome::Close(registry.native_close(view))
            }
            AppWindowEvent::WindowReleased { view } => {
                NativeOutcome::Released(registry.native_released(view))
            }
            AppWindowEvent::WindowBounds {
                view,
                x,
                y,
                width,
                height,
            } => NativeOutcome::Fact(registry.native_bounds(
                view,
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
            )),
            AppWindowEvent::WindowFocus { view, focused } => {
                NativeOutcome::Fact(registry.native_focus(view, focused))
            }
            AppWindowEvent::WindowMaximized { view, maximized } => {
                NativeOutcome::Fact(registry.native_maximized(view, maximized))
            }
            AppWindowEvent::WindowFullScreen { view, full_screen } => {
                NativeOutcome::Fact(registry.native_full_screen(view, full_screen))
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
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::DialogFinished { .. } => {
                return Err(app_detail(
                    "window native event",
                    "dialog completion must be dispatched by the event coordinator",
                ));
            }
        })
    }
}
