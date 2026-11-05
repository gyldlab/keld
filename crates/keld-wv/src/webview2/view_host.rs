//! UI-thread native parent obligations, shared by hello and application views.

use tao::dpi::PhysicalSize;
use tao::event::WindowEvent;
use tao::event_loop::EventLoopProxy;
use tao::platform::windows::WindowExtWindows as _;
use webview2_com::FocusChangedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC;
use windows::Win32::Foundation::{E_UNEXPECTED, HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;

use super::{ICoreWebView2Controller, View, WindowsLoopEvent};
use crate::{WebviewId, WvError};

#[cfg(all(feature = "media-acceptance", test))]
pub(super) mod test_control;

impl View {
    /// Activation belongs to the top-level HWND even when a `WebView` child has
    /// keyboard focus. Read only on the window's UI thread.
    pub(super) fn is_active(&self) -> bool {
        // SAFETY: GetActiveWindow observes only this thread's active window.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getactivewindow
        (unsafe { GetActiveWindow() }) == HWND(self.window.hwnd() as _)
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
        let _ = match event {
            WindowEvent::Moved(_) => {
                // SAFETY: retained controller on its creation/UI thread. This is
                // a parent-position notification, not a bounds or navigation change.
                // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#notifyparentwindowpositionchanged
                unsafe { self.controller.NotifyParentWindowPositionChanged() }
            }
            WindowEvent::Resized(size) => self.resize_for_host(*size),
            WindowEvent::Focused(true) => self.return_focus_if_active(),
            _ => Ok(()),
        };
    }

    fn resize_for_host(&self, size: PhysicalSize<u32>) -> windows::core::Result<()> {
        // Tao0.35.3 queries IsIconic, rather than inferring minimize from 0x0.
        if self.window.is_minimized() {
            // SAFETY: BOOL by value; controller remains owned on this UI thread.
            // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#get_isvisible
            #[cfg(all(feature = "media-acceptance", test))]
            test_control::before(test_control::Step::Hide)?;
            unsafe { self.controller.SetIsVisible(false) }?;
            self.hidden_for_minimize.set(true);
            return Ok(());
        }
        self.set_controller_bounds(size)?;
        if self.hidden_for_minimize.get() {
            // Bounds must be restored before the compositor is made visible.
            // SAFETY: same retained UI-thread controller; no asynchronous result borrow.
            #[cfg(all(feature = "media-acceptance", test))]
            test_control::before(test_control::Step::Show)?;
            unsafe { self.controller.SetIsVisible(true) }?;
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
        unsafe { self.controller.SetBounds(rect) }
    }

    pub(super) fn return_focus_if_active(&self) -> windows::core::Result<()> {
        // SAFETY: thread-local activation query. A queued Focused(true) is not
        // authority to activate this window after a different HWND became active.
        // Keyboard focus may be on the WebView child without deactivating its parent.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getactivewindow
        if !self.is_active() {
            return Ok(());
        }
        // SAFETY: controller, top-level window and active-window query belong to
        // this UI thread. MoveFocus returns keyboard focus, never calls activation.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#movefocus
        #[cfg(all(feature = "media-acceptance", test))]
        test_control::before(test_control::Step::Focus)?;
        unsafe {
            self.controller
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
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
