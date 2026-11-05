//! Test-owned Win32 stimuli and independent state. Extends the reviewed HWND
//! observer; it never installs a `WndProc` or changes production window policy.

use std::ptr;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    SetThreadDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GUITHREADINFO, GetClientRect,
    GetForegroundWindow, GetGUIThreadInfo, IsChild, IsIconic, IsZoomed, MSG, PM_REMOVE,
    PeekMessageW, SW_MINIMIZE, SW_RESTORE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

use super::NativeWindowSnapshot;

#[path = "native_caption_click.rs"]
mod native_caption_click;

/// `GetWindowRect` otherwise reports the caller's virtualized coordinate space.
/// This guard changes only the native test thread and restores its prior context.
pub(crate) struct PhysicalCoordinates(DPI_AWARENESS_CONTEXT);

impl PhysicalCoordinates {
    pub(crate) fn enter() -> Self {
        // SAFETY: a thread-local DPI context, no window or borrowed pointer.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        assert!(
            !previous.is_null(),
            "set test-thread DPI context: {}",
            std::io::Error::last_os_error()
        );
        Self(previous)
    }
}

impl Drop for PhysicalCoordinates {
    fn drop(&mut self) {
        // SAFETY: restore the value returned on this same test thread. Raw handle
        // fields keep this guard non-Send; it never changes another thread.
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

#[derive(Debug)]
pub(crate) struct NativeState {
    pub(crate) window: NativeWindowSnapshot,
    pub(crate) dpi: u32,
    pub(crate) scale: f64,
    pub(crate) client: [i32; 2],
    pub(crate) minimized: bool,
    pub(crate) maximized: bool,
    pub(crate) foreground: usize,
    pub(crate) focus: usize,
    pub(crate) focus_is_child: bool,
}

fn target(hwnd: usize, pid: u32) -> HWND {
    // Observation validates the known fixture process, not new authority over a
    // numeric HWND. The scenario retains that original Host and does not close it
    // while these stimuli run; a changed/foreign owner is refused.
    NativeWindowSnapshot::capture(hwnd as u64, pid).expect("live fixture-owned native target");
    hwnd as HWND
}

pub(crate) fn read(hwnd: usize, pid: u32) -> NativeState {
    let window =
        NativeWindowSnapshot::capture(hwnd as u64, pid).expect("live fixture native state");
    let raw = hwnd as HWND;
    let mut client = RECT::default();
    // SAFETY: read the retained fixture's client rectangle into this initialized
    // stack value. It neither posts messages nor changes the window.
    assert_ne!(
        unsafe { GetClientRect(raw, &raw mut client) },
        0,
        "native client rectangle"
    );
    // SAFETY: numeric read-only query of the retained fixture window.
    let dpi = unsafe { GetDpiForWindow(raw) };
    assert!(dpi > 0, "GetDpiForWindow failed for the live fixture");
    let mut gui = GUITHREADINFO {
        cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).expect("GUI info size"),
        ..GUITHREADINFO::default()
    };
    // SAFETY: thread 0 means the foreground GUI thread. The initialized output
    // lives across this synchronous read, with no cross-process write.
    let gui_present = unsafe { GetGUIThreadInfo(0, &raw mut gui) } != 0;
    let focus = if gui_present {
        gui.hwndFocus
    } else {
        ptr::null_mut()
    };
    // SAFETY: all calls below read numeric HWND state; a recycled handle can fail
    // the scenario's equality/liveness checks but cannot authorize a mutation.
    let (minimized, maximized, foreground, focus_is_child) = unsafe {
        (
            IsIconic(raw) != 0,
            IsZoomed(raw) != 0,
            GetForegroundWindow() as usize,
            IsChild(raw, focus) != 0,
        )
    };
    NativeState {
        window,
        dpi,
        scale: f64::from(dpi) / 96.0,
        client: [client.right - client.left, client.bottom - client.top],
        minimized,
        maximized,
        foreground,
        focus: focus as usize,
        focus_is_child,
    }
}

pub(crate) fn move_by(hwnd: usize, pid: u32, dx: i32, dy: i32) {
    let raw = target(hwnd, pid);
    let before = NativeWindowSnapshot::capture(hwnd as u64, pid).expect("native pre-move frame");
    let x = before.outer_rect[0]
        .checked_add(dx)
        .expect("move X fits Win32 coordinate");
    let y = before.outer_rect[1]
        .checked_add(dy)
        .expect("move Y fits Win32 coordinate");
    // SAFETY: the scenario retains this known live window. Pointer-free movement
    // changes neither size nor activation/z-order, as required by AC26(a).
    assert_ne!(
        unsafe {
            SetWindowPos(
                raw,
                ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        },
        0,
        "native fixture move"
    );
}

pub(crate) fn resize_by(hwnd: usize, pid: u32, dw: i32, dh: i32) {
    let raw = target(hwnd, pid);
    let before = NativeWindowSnapshot::capture(hwnd as u64, pid).expect("native pre-resize frame");
    let width = before.outer_rect[2]
        .checked_sub(before.outer_rect[0])
        .and_then(|value| value.checked_add(dw))
        .expect("native width");
    let height = before.outer_rect[3]
        .checked_sub(before.outer_rect[1])
        .and_then(|value| value.checked_add(dh))
        .expect("native height");
    assert!(width > 0 && height > 0, "positive native resize");
    // SAFETY: pointer-free size change to the original retained fixture window;
    // no move, activation, monitor setting, or foreign resource is changed.
    assert_ne!(
        unsafe {
            SetWindowPos(
                raw,
                ptr::null_mut(),
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        },
        0,
        "native fixture resize"
    );
}

pub(crate) fn minimize(hwnd: usize, pid: u32) {
    let raw = target(hwnd, pid);
    // SAFETY: affects only the original live fixture. Return is prior visibility,
    // not success; IsIconic and page visibility independently prove the outcome.
    unsafe { ShowWindow(raw, SW_MINIMIZE) };
}

pub(crate) fn restore(hwnd: usize, pid: u32) {
    let raw = target(hwnd, pid);
    // SAFETY: same retained target. The scenario proves actual restore state.
    unsafe { ShowWindow(raw, SW_RESTORE) };
}

pub(crate) fn foreground(hwnd: usize, pid: u32) {
    let raw = target(hwnd, pid);
    // SAFETY: asks only to activate a retained fixture window. Windows may deny
    // this; no AttachThreadInput, global foreground grant or policy change exists.
    unsafe { SetForegroundWindow(raw) };
}

/// A different-process native comparator: this guard owns a built-in system
/// HWND while the product Host owns the target. No second webview exists.
pub(crate) struct FocusComparator(HWND);

impl FocusComparator {
    pub(crate) fn new() -> Self {
        Self::with_system_class("STATIC\0")
    }

    pub(crate) fn for_caption_input() -> Self {
        Self::with_system_class("BUTTON\0")
    }

    fn with_system_class(class_name: &'static str) -> Self {
        let class: Vec<_> = class_name.encode_utf16().collect();
        let title: Vec<_> = "KELD AC26 owned focus comparator\0"
            .encode_utf16()
            .collect();
        // SAFETY: fixed built-in STATIC/BUTTON class, initialized NUL-terminated strings and
        // null optional owners. Created and destroyed on this one test thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                800,
                100,
                300,
                200,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        assert!(
            !hwnd.is_null(),
            "create owned focus comparator: {}",
            std::io::Error::last_os_error()
        );
        Self(hwnd)
    }

    pub(crate) fn hwnd(&self) -> usize {
        self.0 as usize
    }

    pub(crate) fn activate(&self) {
        // SAFETY: this thread owns the original live comparator HWND.
        unsafe { SetForegroundWindow(self.0) };
        pump();
    }
}

impl Drop for FocusComparator {
    fn drop(&mut self) {
        // SAFETY: destroy exactly the built-in window created by this guard on
        // this thread. No product HWND or other process is touched.
        unsafe { DestroyWindow(self.0) };
    }
}

pub(crate) fn pump() {
    let mut message = MSG::default();
    // SAFETY: drain only this test thread's queue into a live initialized MSG.
    // Dispatch uses the existing system class procedure, never a new WndProc.
    while unsafe { PeekMessageW(&raw mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
        unsafe {
            TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}
