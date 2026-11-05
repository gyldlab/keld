//! Native facts use the shared view-host owner and the existing Core channel.
use super::super::WindowsLoopEvent;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::super::managed_windows::Releasing;
use super::super::managed_windows::StageResult;
use super::super::view_build::snapshot;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::super::{managed_test_control::TestControl, t2_test_observation as observe};
use super::{ManagedWindows, send};
use crate::engine::AppWindowEvent;
use crate::{WebviewId, WvError};
use tao::event::WindowEvent;
use tao::platform::windows::WindowExtWindows as _;
use tao::window::WindowId;

/// The native facts refreshed by this event. Core still owns state filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeFacts {
    None,
    Focus,
    QueuedFocus,
    BoundsAndFullScreen { maximized: bool },
}

/// Keep event selection in one production owner. The minimized observation is
/// lazy so focus and unrelated events do not gain a native geometry query.
fn native_facts(event: &WindowEvent<'_>, minimized: impl FnOnce() -> bool) -> NativeFacts {
    match event {
        WindowEvent::Focused(_) => NativeFacts::Focus,
        WindowEvent::Moved(_)
        | WindowEvent::Resized(_)
        | WindowEvent::ScaleFactorChanged { .. } => {
            if minimized() {
                NativeFacts::QueuedFocus
            } else {
                NativeFacts::BoundsAndFullScreen {
                    maximized: matches!(event, WindowEvent::Resized(_)),
                }
            }
        }
        _ => NativeFacts::None,
    }
}

impl ManagedWindows {
    fn stage_destroyed(&mut self, window_id: WindowId) {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let entry = self.custody.entry(window_id);
        match self.custody.stage_destroyed(window_id, || {
            self.proxy
                .send_event(WindowsLoopEvent::ProfileReleaseWake)
                .is_ok()
        }) {
            StageResult::Unknown => {
                // Terminal/fault drops owe no individual answer. Unknown native
                // events never gain authority over any live/successor window.
                eprintln!("KELD_WV_DESTROYED_UNOWNED {window_id:?}");
            }
            StageResult::AlreadyStaged => {}
            StageResult::Staged { .. } =>
            {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if let Some((hwnd, owed)) = entry {
                    match owed {
                        Releasing::Destroy(_) => observe::destroyed(hwnd),
                        Releasing::FailedBuild { build, .. } => {
                            if observe::enabled()
                                || self
                                    .test_control
                                    .as_ref()
                                    .is_some_and(|control| control.observes_failed_build(build))
                            {
                                eprintln!("FAILED_BUILD_DESTROYED {hwnd}");
                            }
                        }
                    }
                }
            }
        }
    }

    pub(in crate::webview2) fn window_event(
        &mut self,
        window_id: WindowId,
        event: &WindowEvent<'_>,
    ) -> Result<(), WvError> {
        if matches!(event, WindowEvent::Destroyed) {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            {
                if self
                    .test_control
                    .as_mut()
                    .is_some_and(|control| !control.dispatch_destroyed(window_id))
                {
                    return Ok(());
                }
            }
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            let held_hwnd = self.custody.entry(window_id).map(|(hwnd, _)| hwnd);
            self.stage_destroyed(window_id);
            // U18: last in the actual Destroyed dispatch, without pumping.
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let (Some(control), Some(hwnd)) = (self.test_control.as_ref(), held_hwnd) {
                control.hold_destroyed(hwnd)?;
            }
            return Ok(());
        }
        if self.custody.contains(window_id) {
            if matches!(event, WindowEvent::CloseRequested) {
                eprintln!("KELD_WV_CLOSE_IGNORED releasing={window_id:?}");
            }
            return Ok(());
        }
        let Some((id, view)) = self
            .views
            .iter()
            .find(|(_, view)| view.window.id() == window_id)
        else {
            if matches!(event, WindowEvent::CloseRequested) {
                eprintln!("KELD_WV_CLOSE_IGNORED unknown={window_id:?}");
            }
            return Ok(());
        };
        let id = WebviewId(*id);
        if matches!(event, WindowEvent::CloseRequested) {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            observe::close_dispatched(view.window.hwnd());
            // Tao already converted WM_CLOSE. Core decides whether to destroy.
            return send(
                &self.events,
                AppWindowEvent::WindowCloseRequested { view: id },
            );
        }
        view.handle_host_window_event(event);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if matches!(event, WindowEvent::Moved(_)) {
            observe::parent_moved(id.0);
        }
        match native_facts(event, || view.window.is_minimized()) {
            NativeFacts::Focus => send(
                &self.events,
                AppWindowEvent::WindowFocus {
                    view: id,
                    focused: view.is_active(),
                },
            ),
            NativeFacts::QueuedFocus => {
                // Minimize can change activation without another Tao Focused
                // event after keyboard focus moved into the WebView child.
                // Reuse the controller notification: its consumer re-finds the
                // live view and samples the Host UI queue's actual activation.
                self.proxy
                    .send_event(WindowsLoopEvent::ControllerFocus(id))
                    .map_err(|_| {
                        WvError::EventLoop(String::from(
                            "failed to queue managed activation observation",
                        ))
                    })
            }
            NativeFacts::BoundsAndFullScreen { maximized } => {
                let observed = snapshot(view)?;
                send(
                    &self.events,
                    AppWindowEvent::WindowBounds {
                        view: id,
                        x: observed.x,
                        y: observed.y,
                        width: observed.width,
                        height: observed.height,
                    },
                )?;
                if maximized {
                    send(
                        &self.events,
                        AppWindowEvent::WindowMaximized {
                            view: id,
                            maximized: observed.maximized,
                        },
                    )?;
                }
                send(
                    &self.events,
                    AppWindowEvent::WindowFullScreen {
                        view: id,
                        full_screen: observed.full_screen,
                    },
                )
            }
            NativeFacts::None => Ok(()),
        }
    }

    pub(in crate::webview2) fn controller_focus(&self, id: WebviewId) -> Result<(), WvError> {
        let Some(view) = self.views.get(&id.0) else {
            return Ok(());
        };
        send(
            &self.events,
            AppWindowEvent::WindowFocus {
                view: id,
                focused: view.is_active(),
            },
        )
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2) fn consume_deferred_destroyed(&mut self) {
        if let Some(key) = self
            .test_control
            .as_mut()
            .and_then(TestControl::take_deferred_destroyed)
        {
            self.stage_destroyed(key);
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2) fn post_deferred_destroyed(
        &mut self,
        terminal_has_run: bool,
        barrier_ready_or_faulted: bool,
    ) -> Result<(), WvError> {
        if let Some(control) = self.test_control.as_mut() {
            control.post_deferred_destroyed(terminal_has_run, barrier_ready_or_faulted)?;
        }
        Ok(())
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2) fn live_webviews(
        &self,
    ) -> impl Iterator<Item = (WebviewId, &super::super::ICoreWebView2)> {
        self.views
            .iter()
            .map(|(id, view)| (WebviewId(*id), &view.webview))
    }
}

#[cfg(test)]
#[path = "tests/observation.rs"]
mod tests;
