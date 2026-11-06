//! UI-thread native parent obligations, shared by hello and application views.

use tao::dpi::PhysicalSize;
use tao::event::WindowEvent;
use tao::event_loop::EventLoopProxy;
use tao::platform::windows::WindowExtWindows as _;
use webview2_com::FocusChangedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC;
use windows::Win32::Foundation::{E_UNEXPECTED, HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::{ICoreWebView2Controller, View, WindowsLoopEvent};
use crate::{WebviewId, WvError};

#[cfg(all(feature = "media-acceptance", test))]
pub(super) mod test_control;

/// Read this UI thread's activation once for a coherent native observation.
pub(super) fn active_window() -> HWND {
    // SAFETY: GetActiveWindow observes only this thread's active window.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getactivewindow
    unsafe { GetActiveWindow() }
}

/// Read system foreground ownership on the window's UI thread for public facts.
pub(super) fn foreground_window() -> HWND {
    // SAFETY: GetForegroundWindow takes no pointers and only reads global window
    // state. Compare its numeric result with retained top-level HWNDs; never
    // dereference it or use it to act on another process's window.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getforegroundwindow
    unsafe { GetForegroundWindow() }
}

impl View {
    /// Activation belongs to the top-level HWND even when a `WebView` child has
    /// keyboard focus. Read only on the window's UI thread.
    pub(super) fn is_active(&self) -> bool {
        self.is_active_at(active_window())
    }

    /// Public focus follows the system foreground window, even when another
    /// input queue still names this HWND as its thread-local active window.
    pub(super) fn is_foreground(&self) -> bool {
        self.is_active_at(foreground_window())
    }

    pub(super) fn is_active_at(&self, active: HWND) -> bool {
        active == HWND(self.window.hwnd() as _)
    }

    /// Sizes the controller to fill the window's client area.
    ///
    /// Best-effort by design: a transient failure during a live resize must
    /// not tear down the event loop, and the next resize event re-applies the
    /// correct bounds anyway.
    pub(super) fn fit_controller(&self, size: PhysicalSize<u32>) {
        let _ = self.resize_for_host(size);
    }

    /// Best-effort host events operate only on this live UI-thread view. A failed
    /// hide/show/bounds call never commits a transition that prevents its retry.
    pub(super) fn handle_host_window_event(&self, event: &WindowEvent<'_>) {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let observed = matches!(
            event,
            WindowEvent::Moved(_) | WindowEvent::Resized(_) | WindowEvent::Focused(_)
        );
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if observed && super::t2_test_observation::enabled() {
            eprintln!(
                "KELD_D16_HOST_EVENT view={:?} event={event:?} hidden_before={}",
                self.test_view_id,
                self.hidden_for_minimize.get()
            );
        }
        let result = match event {
            WindowEvent::Moved(_) => {
                // SAFETY: retained controller on its creation/UI thread. This is
                // a parent-position notification, not a bounds or navigation change.
                // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#notifyparentwindowpositionchanged
                unsafe { self.controller.NotifyParentWindowPositionChanged() }.inspect(|&()| {
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    if let Some(id) = self.test_view_id {
                        super::t2_test_observation::parent_moved(id.0);
                    }
                })
            }
            WindowEvent::Resized(size) => self.resize_for_host(*size),
            WindowEvent::Focused(true) => self.return_focus_if_active(),
            _ => Ok(()),
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if observed {
            self.trace_host_step("event.return", Some(&result));
        }
        let _ = result;
    }

    fn resize_for_host(&self, size: PhysicalSize<u32>) -> windows::core::Result<()> {
        // Tao0.35.3 queries IsIconic, rather than inferring minimize from 0x0.
        let minimized = self.window.is_minimized();
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if super::t2_test_observation::enabled() {
            eprintln!(
                "KELD_D16_RESIZE view={:?} size={size:?} minimized={minimized} hidden_before={}",
                self.test_view_id,
                self.hidden_for_minimize.get()
            );
        }
        if minimized {
            // SAFETY: BOOL by value; controller remains owned on this UI thread.
            // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#get_isvisible
            #[cfg(all(feature = "media-acceptance", test))]
            test_control::before(test_control::Step::Hide)?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.trace_host_step("hide.enter", None);
            // SAFETY: unchanged visibility value on the same retained UI-thread controller.
            let result = unsafe { self.controller.SetIsVisible(false) };
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.trace_host_step("hide.return", Some(&result));
            result?;
            self.hidden_for_minimize.set(true);
            return Ok(());
        }
        self.set_controller_bounds(size)?;
        if self.hidden_for_minimize.get() {
            // Bounds must be restored before the compositor is made visible.
            // SAFETY: same retained UI-thread controller; no asynchronous result borrow.
            #[cfg(all(feature = "media-acceptance", test))]
            test_control::before(test_control::Step::Show)?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.trace_host_step("show.enter", None);
            // SAFETY: unchanged visibility value on the same retained UI-thread controller.
            let result = unsafe { self.controller.SetIsVisible(true) };
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.trace_host_step("show.return", Some(&result));
            result?;
            self.return_focus_if_active()?;
            self.hidden_for_minimize.set(false);
        }
        Ok(())
    }

    pub(super) fn set_controller_bounds(
        &self,
        size: PhysicalSize<u32>,
    ) -> windows::core::Result<()> {
        let rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(size.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(size.height).unwrap_or(i32::MAX),
        };
        // SAFETY: plain RECT value and controller retained on its UI thread.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#put_bounds
        #[cfg(all(feature = "media-acceptance", test))]
        test_control::before(test_control::Step::Bounds)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.trace_host_step("bounds.enter", None);
        // SAFETY: the same retained UI-thread controller and plain initialized RECT.
        let result = unsafe { self.controller.SetBounds(rect) };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.trace_host_step("bounds.return", Some(&result));
        result?;
        Ok(())
    }

    pub(super) fn return_focus_if_active(&self) -> windows::core::Result<()> {
        // SAFETY: thread-local activation query. A queued Focused(true) is not
        // authority to activate this window after a different HWND became active.
        // Keyboard focus may be on the WebView child without deactivating its parent.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getactivewindow
        let active = self.is_active();
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if super::t2_test_observation::enabled() {
            eprintln!(
                "KELD_D16_FOCUS view={:?} active={active} hidden_before={}",
                self.test_view_id,
                self.hidden_for_minimize.get()
            );
        }
        if !active {
            return Ok(());
        }
        // SAFETY: controller, top-level window and active-window query belong to
        // this UI thread. MoveFocus returns keyboard focus, never calls activation.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#movefocus
        #[cfg(all(feature = "media-acceptance", test))]
        test_control::before(test_control::Step::Focus)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.trace_host_step("focus.enter", None);
        // SAFETY: the same retained UI-thread controller and unchanged focus reason.
        let result = unsafe {
            self.controller
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.trace_host_step("focus.return", Some(&result));
        result?;
        Ok(())
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn trace_host_step(&self, step: &str, result: Option<&windows::core::Result<()>>) {
        if super::t2_test_observation::enabled() {
            let code = result.map(|value| value.as_ref().err().map_or(0, |error| error.code().0));
            eprintln!(
                "KELD_D16_HOST_STEP view={:?} step={step} hidden={} result={code:?}",
                self.test_view_id,
                self.hidden_for_minimize.get()
            );
        }
    }
}

pub(super) fn install_controller_focus(
    controller: &ICoreWebView2Controller,
    id: WebviewId,
    proxy: &EventLoopProxy<WindowsLoopEvent>,
) -> Result<(), WvError> {
    for gained in [true, false] {
        let proxy = proxy.clone();
        let handler = FocusChangedEventHandler::create(Box::new(move |_, _| {
            proxy
                .send_event(WindowsLoopEvent::ControllerFocus(id))
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
        }));
        let mut token = 0;
        // SAFETY: controller and both callbacks are retained on the UI thread.
        // Close stops controller event callbacks before the Window is dropped.
        let result = unsafe {
            if gained {
                controller.add_GotFocus(&handler, &raw mut token)
            } else {
                controller.add_LostFocus(&handler, &raw mut token)
            }
        };
        result.map_err(|error| WvError::Webview(format!("controller focus observer: {error}")))?;
    }
    Ok(())
}
