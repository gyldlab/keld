//! Fixture-only initial activation: inspect the Host UI thread directly and
//! temporarily prevent first-show activation of one hidden construction HWND.

use super::NativeWindowSnapshot;
use std::ptr;
use windows_sys::Win32::Foundation::{GetLastError, HWND, SetLastError};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GUITHREADINFO, GWL_EXSTYLE, GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId,
    IsChild, IsWindowVisible, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetWindowLongW, SetWindowPos, WS_EX_NOACTIVATE,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ThreadFocus {
    pub(crate) thread: u32,
    pub(crate) active: usize,
    pub(crate) focus: usize,
    pub(crate) focus_is_child: bool,
}

fn owner(hwnd: usize, pid: u32) -> (HWND, u32) {
    NativeWindowSnapshot::capture(hwnd as u64, pid).expect("retained fixture window");
    let mut actual = 0;
    // SAFETY: read the retained fixture HWND's creating thread and process only.
    let thread = unsafe { GetWindowThreadProcessId(hwnd as HWND, &raw mut actual) };
    assert_ne!(thread, 0, "fixture UI thread exists");
    assert_eq!(
        actual, pid,
        "fixture UI thread belongs to the retained Host"
    );
    (hwnd as HWND, thread)
}

pub(crate) fn read(hwnd: usize, pid: u32) -> ThreadFocus {
    let (raw, thread) = owner(hwnd, pid);
    let mut gui = GUITHREADINFO {
        cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).expect("GUI info size"),
        ..GUITHREADINFO::default()
    };
    // SAFETY: read the Host's actual UI queue, not the foreground thread (0).
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getguithreadinfo
    assert_ne!(
        unsafe { GetGUIThreadInfo(thread, &raw mut gui) },
        0,
        "Host GUI thread state"
    );
    ThreadFocus {
        thread,
        active: gui.hwndActive as usize,
        focus: gui.hwndFocus as usize,
        // SAFETY: read-only relationship query between numeric native handles.
        focus_is_child: unsafe { IsChild(raw, gui.hwndFocus) } != 0,
    }
}

fn style(raw: HWND) -> i32 {
    // SAFETY: GWL_EXSTYLE is a bitfield read, never a pointer or window procedure.
    unsafe { SetLastError(0) };
    let value = unsafe { GetWindowLongW(raw, GWL_EXSTYLE) };
    assert!(
        value != 0 || unsafe { GetLastError() } == 0,
        "read fixture extended style"
    );
    value
}

fn write_style(raw: HWND, value: i32) {
    // SAFETY: only the retained fixture HWND's extended style is changed.
    // No WndProc, owner, parent, process, desktop policy or input attachment.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowlongw
    unsafe { SetLastError(0) };
    let previous = unsafe { SetWindowLongW(raw, GWL_EXSTYLE, value) };
    assert!(
        previous != 0 || unsafe { GetLastError() } == 0,
        "write fixture extended style"
    );
    // SAFETY: refresh the same frame without movement, resize, z-order or activation.
    assert_ne!(
        unsafe {
            SetWindowPos(
                raw,
                ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            )
        },
        0,
        "refresh fixture style"
    );
}

fn enabled(raw: HWND) -> bool {
    // SAFETY: read only the retained fixture HWND's enabled state.
    unsafe { IsWindowEnabled(raw) != 0 }
}

fn write_enabled(raw: HWND, value: bool) {
    // SAFETY: change only the retained fixture HWND's input-enabled state.
    // The approved build hold continues pumping sent WM_CANCELMODE/WM_ENABLE.
    // A disabled window cannot activate; the return value is the prior state.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enablewindow
    unsafe { EnableWindow(raw, i32::from(value)) };
    assert_eq!(enabled(raw), value, "fixture enabled state");
}

/// The original Host remains retained, and no successor build is dispatched while
/// this guard exists. Normal restoration is explicit and checked before teardown.
pub(crate) struct NoActivate {
    hwnd: usize,
    pid: u32,
    original: i32,
    original_enabled: bool,
    restore_pending: bool,
}

impl NoActivate {
    pub(crate) fn hidden(hwnd: usize, pid: u32) -> Self {
        let (raw, _) = owner(hwnd, pid);
        // SAFETY: the approved build-pump hold precedes the product's first show.
        assert_eq!(
            unsafe { IsWindowVisible(raw) },
            0,
            "only a hidden held construction may change"
        );
        let original = style(raw);
        let original_enabled = enabled(raw);
        let flag = i32::try_from(WS_EX_NOACTIVATE).expect("NOACTIVATE fits native style");
        // Own restoration before the first native mutation can succeed.
        let guard = Self {
            hwnd,
            pid,
            original,
            original_enabled,
            restore_pending: true,
        };
        write_style(raw, original | flag);
        assert_eq!(style(raw), original | flag, "one fixture style bit only");
        // Tao 0.35.3 explicitly uses SW_SHOW, then rewrites both style fields.
        // Keep the hidden HWND disabled for that first show. Tao may re-enable
        // it afterward, but its frame refresh uses SWP_NOACTIVATE and the
        // product's following MoveFocus remains conditional on actual activation.
        write_enabled(raw, false);
        guard
    }

    pub(crate) fn restore(&mut self) {
        let (raw, _) = owner(self.hwnd, self.pid);
        // This read follows the existing post-Created native focus observation
        // and precedes restoration. Tao may already have restored the enabled
        // state after ShowWindow; it must not be inferred from the style guard.
        let before_restore_enabled = enabled(raw);
        println!(
            "KELD_GH1188_AC13_ENABLE hwnd={} original_enabled={} before_restore_enabled={before_restore_enabled}",
            self.hwnd, self.original_enabled,
        );
        // Restore only the owned extended-style and enabled-state changes.
        // Never overwrite the current normal style with the hidden preimage.
        write_style(raw, self.original);
        write_enabled(raw, self.original_enabled);
        assert_eq!(
            style(raw),
            self.original,
            "original native extended style restored"
        );
        self.restore_pending = false;
    }
}

impl Drop for NoActivate {
    fn drop(&mut self) {
        if !self.restore_pending {
            return;
        }
        // Error cleanup only. The sole original Host and no-successor constraint
        // remain held by this scenario; a dead/foreign HWND is never modified.
        if NativeWindowSnapshot::capture(self.hwnd as u64, self.pid).is_ok() {
            let raw = self.hwnd as HWND;
            // SAFETY: restore only the original extended style and enabled state
            // on this scenario's still-live HWND; normal teardown checks both.
            unsafe {
                SetWindowLongW(raw, GWL_EXSTYLE, self.original);
                EnableWindow(raw, i32::from(self.original_enabled));
            };
        }
    }
}
