//! Apply each native receipt once in the existing Registry before publication.

use super::request::WindowRefusal;
use super::{
    AppWindowEvent, BuildFailureOutcome, BuildOutcome, BuildStage, CloseOutcome, FactOutcome,
    HostAppError, OwnedWindowEvent, PrimaryRouterHandle, Rect, ReleaseOutcome, WindowSession,
    app_detail, native_state,
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

pub(super) fn build_refusal(stage: BuildStage) -> WindowRefusal {
    let message = match stage {
        BuildStage::Window => {
            "KELD-CORE-041: native window creation failed. Inspect the UI owner's typed error and correct the requested window resources."
        }
        BuildStage::Webview => {
            "KELD-CORE-041: native webview creation failed. Inspect the UI owner's typed error and correct the requested window resources."
        }
    };
    WindowRefusal::fixed("KELD-CORE-041", message)
}

pub(super) fn report_ignored_build() {
    eprintln!(
        "KELD-CORE-041: ignored unexpected native build result; inspect the UI build correlation"
    );
}

pub(super) fn report_ignored_build_failure() {
    eprintln!(
        "KELD-CORE-041: ignored unexpected native build failure; inspect the UI build correlation"
    );
}

pub(super) fn report_release(outcome: &ReleaseOutcome) {
    if outcome.diagnostic {
        eprintln!("KELD-CORE-038: unexpected native release; inspect UI teardown ordering");
    }
    if outcome.dropped_facts != 0 {
        eprintln!(
            "window teardown dropped {} native facts",
            outcome.dropped_facts
        );
    }
}

pub(super) fn report_native_close(
    outcome: CloseOutcome,
) -> Result<Option<OwnedWindowEvent>, HostAppError> {
    match outcome {
        CloseOutcome::Requested { fact, .. } => Ok(fact),
        CloseOutcome::Unmapped => {
            eprintln!(
                "KELD-CORE-038: native close references an unmapped view; inspect UI event ordering"
            );
            Ok(None)
        }
        CloseOutcome::Exhausted => {
            eprintln!(
                "KELD-CORE-039: native close_seq exhausted; destroy the window or end the session"
            );
            Ok(None)
        }
        CloseOutcome::AlreadyClosing | CloseOutcome::Merged { .. } => Ok(None),
        CloseOutcome::Teardown(_) | CloseOutcome::Vetoed | CloseOutcome::Stale => Err(app_detail(
            "window native close",
            "registry returned a call-only close outcome",
        )),
    }
}

pub(super) fn report_native_fact(fact: FactOutcome) -> Option<OwnedWindowEvent> {
    match fact {
        FactOutcome::Event(fact) => Some(fact),
        FactOutcome::Unmapped => {
            eprintln!(
                "KELD-CORE-038: native state references an unmapped view; inspect UI event ordering"
            );
            None
        }
        FactOutcome::Coalesced | FactOutcome::Suppressed | FactOutcome::Dropped => None,
    }
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
            #[cfg(windows)]
            AppWindowEvent::DesktopObserved { .. }
            | AppWindowEvent::WindowsDesktopObserved { .. } => {
                return Err(app_detail(
                    "window native event",
                    "desktop observations belong to the current-reader coordinator",
                ));
            }
            #[cfg(any(target_os = "macos", windows))]
            AppWindowEvent::DialogFinished { .. } => {
                return Err(app_detail(
                    "window native event",
                    "dialog completion must be dispatched by the event coordinator",
                ));
            }
            #[cfg(windows)]
            AppWindowEvent::HostWriterProgress => {
                return Err(app_detail(
                    "window native event",
                    "writer progress must be dispatched by the event coordinator",
                ));
            }
        })
    }
}
